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
    pub source_key: Option<String>,
    pub parent_source_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PluginInteractionDraft {
    pub source_key: String,
    pub target_key: String,
    pub kind: Option<String>,
    pub label: String,
    pub note: String,
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

            let columns_body =
                if let (Some(open), Some(close)) = (trimmed.find('('), trimmed.rfind(')')) {
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
                source_key: None,
                parent_source_key: None,
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

pub struct LlmImportPlugin;

impl LlmImportPlugin {
    fn normalize_path_key(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        let normalized = format!("/{}", trimmed.trim_matches('/'));
        if normalized == "/" {
            None
        } else {
            Some(normalized)
        }
    }

    fn parent_key_from_path(path_key: &str) -> Option<String> {
        let segments = path_key
            .split('/')
            .filter(|segment| !segment.trim().is_empty())
            .collect::<Vec<_>>();

        if segments.len() <= 1 {
            None
        } else {
            Some(format!("/{}", segments[..segments.len() - 1].join("/")))
        }
    }
}

impl CatalogPlugin for LlmImportPlugin {
    fn definition(&self) -> PluginDefinition {
        PluginDefinition {
            name: "plugin.llm",
            display_name: "LLM Import Plugin",
            input_type: PluginInputType::FileSystem,
            system_type: "",
        }
    }

    fn transform_file(&self, input_path: &Path) -> Result<Vec<PluginSystemDraft>> {
        let text = std::fs::read_to_string(input_path)?;
        let value: Value = serde_json::from_str(text.as_str())?;

        let systems = if let Some(array) = value.as_array() {
            array.clone()
        } else if let Some(array) = value.get("systems").and_then(|value| value.as_array()) {
            array.clone()
        } else {
            Vec::new()
        };

        let mut drafts = Vec::new();
        for item in systems {
            let Some(object) = item.as_object() else {
                continue;
            };

            let path_key = object
                .get("path")
                .and_then(|value| value.as_str())
                .and_then(Self::normalize_path_key);

            let source_key = object
                .get("key")
                .and_then(|value| value.as_str())
                .and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_owned())
                    }
                })
                .or(path_key.clone());

            let parent_source_key = object
                .get("parentKey")
                .and_then(|value| value.as_str())
                .and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_owned())
                    }
                })
                .or_else(|| path_key.as_deref().and_then(Self::parent_key_from_path));

            let inferred_name_from_path = path_key.as_deref().and_then(|path| {
                path.split('/')
                    .filter(|segment| !segment.trim().is_empty())
                    .next_back()
                    .map(str::to_owned)
            });

            let name = object
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .or(inferred_name_from_path)
                .or(source_key.clone())
                .unwrap_or_else(|| "system".to_owned());

            let description = object
                .get("description")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .unwrap_or("Imported from LLM")
                .to_owned();

            let system_type = object
                .get("systemType")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("service")
                .to_owned();

            let route_methods = if let Some(array) = object
                .get("routeMethods")
                .and_then(|value| value.as_array())
            {
                let methods = array
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_uppercase())
                    .collect::<Vec<_>>();
                if methods.is_empty() {
                    None
                } else {
                    Some(methods.join(","))
                }
            } else {
                object
                    .get("routeMethods")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            };

            drafts.push(PluginSystemDraft {
                name,
                description,
                system_type,
                route_methods,
                database_columns: Vec::new(),
                source_key,
                parent_source_key,
            });
        }

        Ok(drafts)
    }
}

pub fn parse_llm_detailed_import_file(
    input_path: &Path,
) -> Result<(Vec<PluginSystemDraft>, Vec<PluginInteractionDraft>)> {
    let text = std::fs::read_to_string(input_path)?;
    let value: Value = serde_json::from_str(text.as_str())?;

    let systems_items = value
        .get("systems")
        .and_then(|systems| systems.as_array())
        .cloned()
        .unwrap_or_default();

    let mut systems = Vec::new();
    for item in systems_items {
        let Some(object) = item.as_object() else {
            continue;
        };

        let path_key = object
            .get("path")
            .and_then(|value| value.as_str())
            .and_then(LlmImportPlugin::normalize_path_key);

        let source_key = object
            .get("key")
            .and_then(|value| value.as_str())
            .and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            })
            .or(path_key.clone());

        let parent_source_key = object
            .get("parentKey")
            .and_then(|value| value.as_str())
            .and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            })
            .or_else(|| {
                path_key
                    .as_deref()
                    .and_then(LlmImportPlugin::parent_key_from_path)
            });

        let inferred_name_from_path = path_key.as_deref().and_then(|path| {
            path.split('/')
                .filter(|segment| !segment.trim().is_empty())
                .next_back()
                .map(str::to_owned)
        });

        let name = object
            .get("name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or(inferred_name_from_path)
            .or(source_key.clone())
            .unwrap_or_else(|| "system".to_owned());

        let description = object
            .get("description")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .unwrap_or("Imported from detailed LLM map")
            .to_owned();

        let system_type = object
            .get("systemType")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("service")
            .to_owned();

        let route_methods = if let Some(array) = object
            .get("routeMethods")
            .and_then(|value| value.as_array())
        {
            let methods = array
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_uppercase())
                .collect::<Vec<_>>();
            if methods.is_empty() {
                None
            } else {
                Some(methods.join(","))
            }
        } else {
            object
                .get("routeMethods")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };

        systems.push(PluginSystemDraft {
            name,
            description,
            system_type,
            route_methods,
            database_columns: Vec::new(),
            source_key,
            parent_source_key,
        });
    }

    let interactions_items = value
        .get("interactions")
        .and_then(|items| items.as_array())
        .cloned()
        .unwrap_or_default();

    let mut interactions = Vec::new();
    for item in interactions_items {
        let Some(object) = item.as_object() else {
            continue;
        };

        let source_key = object
            .get("sourceKey")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        let target_key = object
            .get("targetKey")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        let (Some(source_key), Some(target_key)) = (source_key, target_key) else {
            continue;
        };

        let kind = object
            .get("kind")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        let label = object
            .get("label")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("LLM inferred interaction")
            .to_owned();

        let note = object
            .get("note")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .unwrap_or("Imported from detailed LLM interaction map")
            .to_owned();

        interactions.push(PluginInteractionDraft {
            source_key,
            target_key,
            kind,
            label,
            note,
        });
    }

    Ok((systems, interactions))
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

        let mut methods_by_path: std::collections::HashMap<String, HashSet<String>> =
            std::collections::HashMap::new();
        let mut all_levels = HashSet::new();

        for (path_name, path_value) in paths {
            let Some(path_methods) = path_value.as_object() else {
                continue;
            };

            let trimmed_path = path_name.trim();
            if trimmed_path.is_empty() {
                continue;
            }

            let normalized_path = format!("/{}", trimmed_path.trim_matches('/').trim())
                .trim_end_matches('/')
                .to_owned();
            if normalized_path == "/" {
                continue;
            }

            let methods = methods_by_path.entry(normalized_path.clone()).or_default();
            for method in &allowed_methods {
                if path_methods.contains_key(*method) {
                    methods.insert(method.to_uppercase());
                }
            }

            let segments = normalized_path
                .split('/')
                .filter(|segment| !segment.trim().is_empty())
                .collect::<Vec<_>>();
            for depth in 1..=segments.len() {
                all_levels.insert(format!("/{}", segments[..depth].join("/")));
            }
        }

        let mut levels = all_levels.into_iter().collect::<Vec<_>>();
        levels.sort_by(|left, right| {
            let left_depth = left
                .split('/')
                .filter(|segment| !segment.is_empty())
                .count();
            let right_depth = right
                .split('/')
                .filter(|segment| !segment.is_empty())
                .count();
            left_depth
                .cmp(&right_depth)
                .then_with(|| left.to_lowercase().cmp(&right.to_lowercase()))
        });

        for level_path in levels {
            let segments = level_path
                .split('/')
                .filter(|segment| !segment.trim().is_empty())
                .collect::<Vec<_>>();
            let Some(name) = segments.last() else {
                continue;
            };

            let parent_source_key = if segments.len() > 1 {
                Some(format!("/{}", segments[..segments.len() - 1].join("/")))
            } else {
                None
            };

            let methods = methods_by_path
                .get(&level_path)
                .map(|set| {
                    allowed_methods
                        .iter()
                        .filter_map(|method| {
                            let upper = method.to_uppercase();
                            if set.contains(upper.as_str()) {
                                Some(upper)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            drafts.push(PluginSystemDraft {
                name: (*name).to_owned(),
                description: format!("Imported from OpenAPI path {}", level_path),
                system_type: "api".to_owned(),
                route_methods: if methods.is_empty() {
                    None
                } else {
                    Some(methods.join(","))
                },
                database_columns: Vec::new(),
                source_key: Some(level_path),
                parent_source_key,
            });
        }

        Ok(drafts)
    }
}

pub fn plugin_by_name(name: &str) -> Option<Box<dyn CatalogPlugin>> {
    match name {
        "plugin.ddl" => Some(Box::new(DdlPlugin)),
        "plugin.openapi" => Some(Box::new(OpenApiPlugin)),
        "plugin.llm" => Some(Box::new(LlmImportPlugin)),
        "plugin.llm.detailed" => Some(Box::new(LlmImportPlugin)),
        _ => None,
    }
}
