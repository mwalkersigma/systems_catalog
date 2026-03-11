param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("major", "minor", "bugfix")]
    [string]$Type
)

$ErrorActionPreference = "Stop"

function Get-CurrentVersion {
    $content = Get-Content -Raw -Path "Cargo.toml"
    $match = [regex]::Match($content, '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"')
    if (-not $match.Success) {
        throw "Could not read semantic version from Cargo.toml"
    }

    return [pscustomobject]@{
        Major = [int]$match.Groups[1].Value
        Minor = [int]$match.Groups[2].Value
        Patch = [int]$match.Groups[3].Value
        Full = $match.Groups[0].Value
    }
}

function Ensure-CleanGitTree {
    git diff --quiet
    if ($LASTEXITCODE -ne 0) {
        throw "Working tree has unstaged changes"
    }

    git diff --cached --quiet
    if ($LASTEXITCODE -ne 0) {
        throw "Working tree has staged but uncommitted changes"
    }
}

Ensure-CleanGitTree

$current = Get-CurrentVersion
$major = $current.Major
$minor = $current.Minor
$patch = $current.Patch

switch ($Type) {
    "major" {
        $major += 1
        $minor = 0
        $patch = 0
    }
    "minor" {
        $minor += 1
        $patch = 0
    }
    "bugfix" {
        $patch += 1
    }
}

$nextVersion = "$major.$minor.$patch"
$tag = "v$nextVersion"

$existingTag = git tag --list $tag
if ($existingTag) {
    throw "Tag $tag already exists"
}

$content = Get-Content -Raw -Path "Cargo.toml"
$updated = [regex]::Replace(
    $content,
    '(?m)^version\s*=\s*"\d+\.\d+\.\d+"',
    "version = `"$nextVersion`"",
    1
)
Set-Content -Path "Cargo.toml" -Value $updated -NoNewline

cargo check

git add Cargo.toml
git commit -m "chore(release): $tag"
git tag $tag
git push
git push origin $tag

Write-Host "Published $tag"
