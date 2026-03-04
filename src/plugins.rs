use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::models::DatabaseColumnInput;

#[derive(Debug, Clone)]
pub enum PluginInputType {
    FileSystem,
}

#[derive(Debug, Clone)]
pub struct PluginDefinition {
    pub name: &'static str,
    pub display_name: &'static str,
    pub input_type: PluginInputType,
    pub system_type: &'static str,
}

#[derive(Debug, Clone)]
pub struct PluginSystemDraft {
    pub name: String,
    pub description: String,
    pub system_type: String,
    pub route_methods: Option<String>,
    pub database_columns: Vec<DatabaseColumnInput>,
}

pub trait CatalogPlugin {
    fn definition(&self) -> PluginDefinition;
    fn transform_file(&self, input_path: &Path) -> Result<Vec<PluginSystemDraft>>;
}

pub struct DdlPlugin;

impl DdlPlugin {
    fn parse_table_name(header: &str) -> Option<String> {
        let normalized = header
            .replace('`', "")
            .replace('"', "")
            .replace('[', "")
            .replace(']', "");
        let lower = normalized.to_lowercase();
        let create_pos = lower.find("create table")?;
        let after = normalized.get(create_pos + "create table".len()..)?.trim();
        let after = after
            .strip_prefix("if not exists")
            .map(str::trim)
            .unwrap_or(after);

        let table_name = after
            .split(|character: char| character == '(' || character.is_whitespace())
            .find(|value| !value.trim().is_empty())?
            .trim()
            .trim_end_matches(';')
            .to_owned();

        if table_name.is_empty() {
            None
        } else {
            Some(table_name)
        }
    }

    fn parse_column_line(line: &str, position: i64) -> Option<DatabaseColumnInput> {
        let trimmed = line.trim().trim_end_matches(',').trim();
        if trimmed.is_empty() {
            return None;
        }

        let lower = trimmed.to_lowercase();
        if lower.starts_with("primary key")
            || lower.starts_with("foreign key")
            || lower.starts_with("constraint")
            || lower.starts_with("unique")
            || lower.starts_with("check")
            || lower.starts_with(")")
        {
            return None;
        }

        let mut parts = trimmed.split_whitespace();
        let column_name = parts.next()?.trim_matches('`').trim_matches('"').to_owned();
        let column_type = parts.next().unwrap_or("text").to_owned();
        let constraints_raw = parts.collect::<Vec<_>>().join(" ");
        let constraints = if constraints_raw.trim().is_empty() {
            None
        } else {
            Some(constraints_raw)
        };

        Some(DatabaseColumnInput {
            position,
            column_name,
            column_type,
            constraints,
        })
    }
}

impl CatalogPlugin for DdlPlugin {
    fn definition(&self) -> PluginDefinition {
        PluginDefinition {
            name: "plugin.ddl",
            display_name: "DDL Plugin",
            input_type: PluginInputType::FileSystem,
            system_type: "database",
        }
    }

    fn transform_file(&self, input_path: &Path) -> Result<Vec<PluginSystemDraft>> {
        let ddl_text = std::fs::read_to_string(input_path)?;
        let mut drafts = Vec::new();

        for statement in ddl_text.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_lowercase();
            if !lower.contains("create table") {
                continue;
            }

            let table_name = Self::parse_table_name(trimmed).unwrap_or_else(|| "table".to_owned());

            let columns_body = if let (Some(open), Some(close)) = (trimmed.find('('), trimmed.rfind(')')) {
                &trimmed[open + 1..close]
            } else {
                ""
            };

            let mut column_position = 0_i64;
            let columns = columns_body
                .lines()
                .filter_map(|line| {
                    let parsed = Self::parse_column_line(line, column_position);
                    if parsed.is_some() {
                        column_position += 1;
                    }
                    parsed
                })
                .collect::<Vec<_>>();

            drafts.push(PluginSystemDraft {
                name: table_name,
                description: "Imported from DDL".to_owned(),
                system_type: "database".to_owned(),
                route_methods: None,
                database_columns: columns,
            });
        }

        Ok(drafts)
    }
}

pub struct OpenApiPlugin;

impl OpenApiPlugin {
    fn parse_openapi_value(path: &Path) -> Result<Value> {
        let text = std::fs::read_to_string(path)?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if extension == "yaml" || extension == "yml" {
            let yaml_value: serde_yaml::Value = serde_yaml::from_str(text.as_str())?;
            Ok(serde_json::to_value(yaml_value)?)
        } else {
            Ok(serde_json::from_str(text.as_str())?)
        }
    }
}

impl CatalogPlugin for OpenApiPlugin {
    fn definition(&self) -> PluginDefinition {
        PluginDefinition {
            name: "plugin.openapi",
            display_name: "OpenAPI Plugin",
            input_type: PluginInputType::FileSystem,
            system_type: "api",
        }
    }

    fn transform_file(&self, input_path: &Path) -> Result<Vec<PluginSystemDraft>> {
        let value = Self::parse_openapi_value(input_path)?;
        let mut drafts = Vec::new();

        let Some(paths) = value.get("paths").and_then(|value| value.as_object()) else {
            return Ok(drafts);
        };

        let allowed_methods = ["get", "post", "put", "patch", "delete", "head", "options"];

        for (path_name, path_value) in paths {
            let Some(path_methods) = path_value.as_object() else {
                continue;
            };

            for method in &allowed_methods {
                if !path_methods.contains_key(*method) {
                    continue;
                }

                let route_name = format!("{} {}", method.to_uppercase(), path_name);
                let mut method_set = HashSet::new();
                method_set.insert(method.to_uppercase());

                drafts.push(PluginSystemDraft {
                    name: route_name,
                    description: "Imported from OpenAPI".to_owned(),
                    system_type: "api".to_owned(),
                    route_methods: Some(method_set.into_iter().collect::<Vec<_>>().join(",")),
                    database_columns: Vec::new(),
                });
            }
        }

        Ok(drafts)
    }
}

pub fn plugin_by_name(name: &str) -> Option<Box<dyn CatalogPlugin>> {
    match name {
        "plugin.ddl" => Some(Box::new(DdlPlugin)),
        "plugin.openapi" => Some(Box::new(OpenApiPlugin)),
        _ => None,
    }
}
