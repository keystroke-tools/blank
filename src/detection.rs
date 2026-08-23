use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
pub struct BuildSuggestions {
    pub repository_mise: bool,
    pub tools: Vec<String>,
    pub install_command: Option<String>,
    pub build_command: Option<String>,
    pub publish_directory: String,
    pub detected_framework: Option<String>,
    pub isolate_package_workspace: bool,
}

#[derive(Deserialize)]
struct PackageJson {
    #[serde(rename = "packageManager")]
    package_manager: Option<String>,
    engines: Option<Engines>,
    scripts: Option<std::collections::HashMap<String, String>>,
    dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    dev_dependencies: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
struct Engines {
    node: Option<String>,
}

pub async fn detect_within(project: &Path, repository_root: &Path) -> Result<BuildSuggestions> {
    let mut result = BuildSuggestions {
        publish_directory: "dist".into(),
        ..Default::default()
    };
    result.repository_mise = project.join("mise.toml").exists()
        || project.join(".mise.toml").exists()
        || project.join(".tool-versions").exists();
    let package_path = project.join("package.json");
    if !package_path.exists() {
        result.publish_directory = ".".into();
        return Ok(result);
    }
    let package: PackageJson = serde_json::from_slice(
        &tokio::fs::read(package_path)
            .await
            .context("failed to read package.json")?,
    )
    .context("package.json is invalid")?;
    let node_version = read_first(project, &[".node-version", ".nvmrc"])
        .await
        .or_else(|| {
            package
                .engines
                .and_then(|engines| engines.node)
                .map(|value| normalize_node(&value))
        })
        .unwrap_or_else(|| "24".into());
    if !result.repository_mise {
        result.tools.push(format!("node@{node_version}"));
    }
    let (manager, mut version, manager_directory) =
        find_package_manager(project, repository_root, package.package_manager.as_deref()).await;
    if manager == "pnpm" && version.is_none() {
        version = Some(inferred_pnpm_version(&manager_directory).await.into());
    }
    result.isolate_package_workspace =
        manager == "pnpm" && !repository_has_pnpm_workspace(&manager_directory, repository_root);
    if !result.repository_mise && manager != "npm" {
        result.tools.push(format!(
            "{manager}@{}",
            version.as_deref().unwrap_or("latest")
        ));
    }
    result.install_command = Some(
        match manager.as_str() {
            "pnpm" => "pnpm install --frozen-lockfile",
            "yarn" => "yarn install --immutable",
            "bun" => "bun install --frozen-lockfile",
            _ => "npm ci",
        }
        .into(),
    );
    if package
        .scripts
        .as_ref()
        .is_some_and(|scripts| scripts.contains_key("build"))
    {
        result.build_command = Some(format!("{manager} run build"));
    }
    let has = |name: &str| {
        package
            .dependencies
            .as_ref()
            .is_some_and(|deps| deps.contains_key(name))
            || package
                .dev_dependencies
                .as_ref()
                .is_some_and(|deps| deps.contains_key(name))
    };
    if has("astro") {
        result.detected_framework = Some("Astro".into());
        result.publish_directory = "dist".into();
    } else if has("vite") {
        result.detected_framework = Some("Vite".into());
        result.publish_directory = "dist".into();
    }
    Ok(result)
}

async fn find_package_manager(
    project: &Path,
    repository_root: &Path,
    local_package_manager: Option<&str>,
) -> (String, Option<String>, std::path::PathBuf) {
    let mut directory = project.to_path_buf();
    let mut declared = local_package_manager.map(str::to_owned);
    loop {
        if declared.is_none() && directory != project {
            declared = tokio::fs::read(directory.join("package.json"))
                .await
                .ok()
                .and_then(|bytes| serde_json::from_slice::<PackageJson>(&bytes).ok())
                .and_then(|package| package.package_manager);
        }
        if let Some(value) = declared.as_deref() {
            let (name, version) = value
                .split_once('@')
                .map(|(name, version)| (name, Some(version.to_owned())))
                .unwrap_or((value, None));
            return (name.to_owned(), version, directory);
        }
        for (file, manager) in [
            ("pnpm-lock.yaml", "pnpm"),
            ("yarn.lock", "yarn"),
            ("bun.lock", "bun"),
            ("bun.lockb", "bun"),
            ("package-lock.json", "npm"),
        ] {
            if directory.join(file).exists() {
                return (manager.into(), None, directory);
            }
        }
        if directory == repository_root
            || !directory.pop()
            || !directory.starts_with(repository_root)
        {
            return ("npm".into(), None, project.to_path_buf());
        }
    }
}

async fn inferred_pnpm_version(project: &Path) -> &'static str {
    let lockfile = tokio::fs::read_to_string(project.join("pnpm-lock.yaml"))
        .await
        .unwrap_or_default();
    if lockfile
        .lines()
        .next()
        .is_some_and(|line| line.contains("'6"))
    {
        "8"
    } else if lockfile
        .lines()
        .next()
        .is_some_and(|line| line.contains("'9"))
    {
        "9"
    } else {
        "10"
    }
}

fn repository_has_pnpm_workspace(project: &Path, repository_root: &Path) -> bool {
    for directory in project.ancestors() {
        if directory.join("pnpm-workspace.yaml").exists() {
            return true;
        }
        if directory == repository_root {
            break;
        }
        if directory.join(".git").exists() {
            break;
        }
    }
    false
}

async fn read_first(project: &Path, names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(value) = tokio::fs::read_to_string(project.join(name)).await {
            let value = value.trim().trim_start_matches('v');
            if !value.is_empty() {
                return Some(value.into());
            }
        }
    }
    None
}

fn normalize_node(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(|character: char| !character.is_ascii_digit())
        .split(['.', ' ', '|'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("24")
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_common_node_ranges() {
        assert_eq!(normalize_node(">=22"), "22");
        assert_eq!(normalize_node("^20.11.0"), "20");
    }

    #[tokio::test]
    async fn detects_pnpm_vite_project() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(
            directory.path().join("package.json"),
            r#"{"packageManager":"pnpm@10.0.0","scripts":{"build":"vite build"},"devDependencies":{"vite":"latest"}}"#,
        )
        .await
        .unwrap();
        tokio::fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9'",
        )
        .await
        .unwrap();
        let result = detect_within(directory.path(), directory.path())
            .await
            .unwrap();
        assert_eq!(result.tools, ["node@24", "pnpm@10.0.0"]);
        assert_eq!(
            result.install_command.as_deref(),
            Some("pnpm install --frozen-lockfile")
        );
        assert_eq!(result.build_command.as_deref(), Some("pnpm run build"));
        assert_eq!(result.detected_framework.as_deref(), Some("Vite"));
        assert!(result.isolate_package_workspace);
    }

    #[tokio::test]
    async fn pins_pnpm_for_an_unversioned_lockfile() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(
            directory.path().join("package.json"),
            r#"{"scripts":{"build":"astro build"}}"#,
        )
        .await
        .unwrap();
        tokio::fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'",
        )
        .await
        .unwrap();
        let result = detect_within(directory.path(), directory.path())
            .await
            .unwrap();
        assert_eq!(result.tools, ["node@24", "pnpm@9"]);
    }

    #[tokio::test]
    async fn finds_monorepo_package_manager_without_inheriting_root_mise() {
        let repository = tempfile::tempdir().unwrap();
        let project = repository.path().join("apps/web");
        tokio::fs::create_dir_all(&project).await.unwrap();
        tokio::fs::write(repository.path().join("mise.toml"), "[tools]\npnpm = '11'")
            .await
            .unwrap();
        tokio::fs::write(
            repository.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'",
        )
        .await
        .unwrap();
        tokio::fs::write(
            project.join("package.json"),
            r#"{"scripts":{"build":"vite build"},"devDependencies":{"vite":"latest"}}"#,
        )
        .await
        .unwrap();

        let result = detect_within(&project, repository.path()).await.unwrap();

        assert!(!result.repository_mise);
        assert_eq!(result.tools, ["node@24", "pnpm@9"]);
        assert_eq!(result.build_command.as_deref(), Some("pnpm run build"));
    }
}
