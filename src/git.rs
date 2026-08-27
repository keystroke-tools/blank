use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use tokio::{process::Command, time::timeout};
use uuid::Uuid;

#[derive(Clone)]
pub struct GitService {
    repositories: PathBuf,
    builds: PathBuf,
    keys: PathBuf,
    timeout: Duration,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RemoteInspection {
    pub default_branch: Option<String>,
    pub branches: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CommitMetadata {
    pub sha: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RepositoryEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
}

#[allow(dead_code)]
pub struct Worktree {
    path: PathBuf,
    repository: PathBuf,
    timeout: Duration,
}

#[allow(dead_code)]
impl Worktree {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn remove(self) -> Result<()> {
        run_git(
            self.timeout,
            [
                OsStr::new("--git-dir"),
                self.repository.as_os_str(),
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                self.path.as_os_str(),
            ],
        )
        .await?;
        Ok(())
    }
}

impl GitService {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            repositories: data_dir.join("repositories"),
            builds: data_dir.join("builds"),
            keys: data_dir.join("keys"),
            timeout: Duration::from_secs(120),
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.repositories)
            .await
            .context("failed to create repository cache directory")?;
        tokio::fs::create_dir_all(&self.builds)
            .await
            .context("failed to create build directory")?;
        tokio::fs::create_dir_all(&self.keys)
            .await
            .context("failed to create key directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&self.keys, std::fs::Permissions::from_mode(0o700)).await?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn inspect_remote(&self, repository_url: &str) -> Result<RemoteInspection> {
        self.inspect_remote_with_token(repository_url, None).await
    }

    pub async fn inspect_remote_with_token(
        &self,
        repository_url: &str,
        token: Option<&str>,
    ) -> Result<RemoteInspection> {
        validate_repository_url(repository_url)?;
        let output = run_git_authenticated(
            self.timeout,
            [
                OsStr::new("ls-remote"),
                OsStr::new("--symref"),
                OsStr::new(repository_url),
                OsStr::new("HEAD"),
                OsStr::new("refs/heads/*"),
            ],
            None,
            &self.keys.join("known_hosts"),
            token,
        )
        .await?;
        parse_ls_remote(&output)
    }

    #[cfg(test)]
    pub async fn fetch(&self, site_id: &str, repository_url: &str) -> Result<PathBuf> {
        self.fetch_with_token(site_id, repository_url, None).await
    }

    pub async fn fetch_with_token(
        &self,
        site_id: &str,
        repository_url: &str,
        token: Option<&str>,
    ) -> Result<PathBuf> {
        validate_identifier(site_id)?;
        validate_repository_url(repository_url)?;
        self.prepare().await?;
        let repository = self.repositories.join(format!("{site_id}.git"));
        if !repository.exists() {
            let temporary = self
                .repositories
                .join(format!(".{site_id}-{}.tmp", Uuid::new_v4()));
            let initialize = async {
                run_git(
                    self.timeout,
                    [
                        OsStr::new("init"),
                        OsStr::new("--bare"),
                        temporary.as_os_str(),
                    ],
                )
                .await?;
                run_git(
                    self.timeout,
                    [
                        OsStr::new("--git-dir"),
                        temporary.as_os_str(),
                        OsStr::new("remote"),
                        OsStr::new("add"),
                        OsStr::new("origin"),
                        OsStr::new(repository_url),
                    ],
                )
                .await?;
                fetch_repository(
                    self.timeout,
                    &temporary,
                    self.deploy_key_path(site_id)
                        .exists()
                        .then(|| self.deploy_key_path(site_id)),
                    &self.keys.join("known_hosts"),
                    token,
                )
                .await?;
                tokio::fs::rename(&temporary, &repository)
                    .await
                    .context("failed to activate repository cache")?;
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if initialize.is_err() {
                let _ = tokio::fs::remove_dir_all(&temporary).await;
            }
            initialize?;
        } else {
            let current_url = run_git(
                self.timeout,
                [
                    OsStr::new("--git-dir"),
                    repository.as_os_str(),
                    OsStr::new("remote"),
                    OsStr::new("get-url"),
                    OsStr::new("origin"),
                ],
            )
            .await?;
            if current_url.trim() != repository_url {
                run_git(
                    self.timeout,
                    [
                        OsStr::new("--git-dir"),
                        repository.as_os_str(),
                        OsStr::new("remote"),
                        OsStr::new("set-url"),
                        OsStr::new("origin"),
                        OsStr::new(repository_url),
                    ],
                )
                .await?;
            }
            fetch_repository(
                self.timeout,
                &repository,
                self.deploy_key_path(site_id)
                    .exists()
                    .then(|| self.deploy_key_path(site_id)),
                &self.keys.join("known_hosts"),
                token,
            )
            .await?;
        }
        Ok(repository)
    }

    pub async fn resolve_commit(&self, site_id: &str, branch: &str) -> Result<CommitMetadata> {
        validate_identifier(site_id)?;
        validate_branch(branch)?;
        let repository = self.repositories.join(format!("{site_id}.git"));
        if !repository.exists() {
            bail!("repository cache does not exist");
        }
        let revision = format!("refs/remotes/origin/{branch}^{{commit}}");
        let format = "%H%x00%s%x00%an%x00%ae%x00%aI";
        let output = run_git(
            self.timeout,
            [
                OsStr::new("--git-dir"),
                repository.as_os_str(),
                OsStr::new("show"),
                OsStr::new("-s"),
                OsStr::new(&format!("--format={format}")),
                OsStr::new(&revision),
            ],
        )
        .await?;
        parse_commit(&output)
    }

    pub async fn cached_branches(&self, site_id: &str) -> Result<Vec<String>> {
        validate_identifier(site_id)?;
        let repository = self.repositories.join(format!("{site_id}.git"));
        let output = run_git(
            self.timeout,
            [
                OsStr::new("--git-dir"),
                repository.as_os_str(),
                OsStr::new("for-each-ref"),
                OsStr::new("--format=%(refname:strip=3)"),
                OsStr::new("refs/remotes/origin"),
            ],
        )
        .await?;
        let mut branches: Vec<_> = output
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        branches.sort();
        Ok(branches)
    }

    pub async fn list_tree(
        &self,
        site_id: &str,
        branch: &str,
        path: &str,
    ) -> Result<Vec<RepositoryEntry>> {
        validate_identifier(site_id)?;
        validate_branch(branch)?;
        validate_tree_path(path)?;
        let repository = self.repositories.join(format!("{site_id}.git"));
        if !repository.exists() {
            bail!("repository cache does not exist");
        }
        let revision = if path.is_empty() || path == "." {
            format!("refs/remotes/origin/{branch}")
        } else {
            format!("refs/remotes/origin/{branch}:{path}")
        };
        let output = run_git(
            self.timeout,
            [
                OsStr::new("--git-dir"),
                repository.as_os_str(),
                OsStr::new("ls-tree"),
                OsStr::new("--format=%(objecttype)%x09%(path)"),
                OsStr::new(&revision),
            ],
        )
        .await?;
        let prefix = if path.is_empty() || path == "." {
            ""
        } else {
            path
        };
        let mut entries = output
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(kind, name)| RepositoryEntry {
                name: name.to_owned(),
                path: if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                },
                kind: kind.to_owned(),
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            (left.kind.as_str() != "tree", &left.name)
                .cmp(&(right.kind.as_str() != "tree", &right.name))
        });
        Ok(entries)
    }

    #[allow(dead_code)]
    pub async fn create_worktree(
        &self,
        site_id: &str,
        deployment_id: &str,
        commit_sha: &str,
    ) -> Result<Worktree> {
        validate_identifier(site_id)?;
        validate_identifier(deployment_id)?;
        validate_sha(commit_sha)?;
        self.prepare().await?;
        let repository = self.repositories.join(format!("{site_id}.git"));
        if !repository.exists() {
            bail!("repository cache does not exist");
        }
        let path = self.builds.join(deployment_id);
        if path.exists() {
            bail!("deployment workspace already exists");
        }
        run_git(
            self.timeout,
            [
                OsStr::new("--git-dir"),
                repository.as_os_str(),
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--detach"),
                path.as_os_str(),
                OsStr::new(commit_sha),
            ],
        )
        .await?;
        Ok(Worktree {
            path,
            repository,
            timeout: self.timeout,
        })
    }

    fn deploy_key_path(&self, site_id: &str) -> PathBuf {
        self.keys.join(site_id)
    }

    pub async fn deploy_key(&self, site_id: &str) -> Result<Option<String>> {
        validate_identifier(site_id)?;
        let public = self.deploy_key_path(site_id).with_extension("pub");
        match tokio::fs::read_to_string(public).await {
            Ok(value) => Ok(Some(value.trim().to_owned())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("failed to read deploy key"),
        }
    }

    pub async fn generate_deploy_key(&self, site_id: &str) -> Result<String> {
        validate_identifier(site_id)?;
        self.prepare().await?;
        let private = self.deploy_key_path(site_id);
        if private.exists() || private.with_extension("pub").exists() {
            bail!("deploy key already exists");
        }
        let output = timeout(
            self.timeout,
            Command::new("ssh-keygen")
                .args([
                    OsStr::new("-q"),
                    OsStr::new("-t"),
                    OsStr::new("ed25519"),
                    OsStr::new("-N"),
                    OsStr::new(""),
                    OsStr::new("-C"),
                    OsStr::new(&format!("blank-site-{site_id}")),
                    OsStr::new("-f"),
                    private.as_os_str(),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env_clear()
                .env("PATH", "/usr/local/bin:/usr/bin:/bin")
                .output(),
        )
        .await
        .context("ssh-keygen timed out")??;
        if !output.status.success() {
            let _ = tokio::fs::remove_file(&private).await;
            let _ = tokio::fs::remove_file(private.with_extension("pub")).await;
            bail!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        self.deploy_key(site_id)
            .await?
            .context("ssh-keygen did not create a public key")
    }

    pub async fn delete_deploy_key(&self, site_id: &str) -> Result<()> {
        validate_identifier(site_id)?;
        let private = self.deploy_key_path(site_id);
        for path in [&private, &private.with_extension("pub")] {
            if let Err(error) = tokio::fs::remove_file(path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(error).context("failed to remove deploy key");
            }
        }
        Ok(())
    }

    pub async fn delete_site_data(&self, site_id: &str) -> Result<()> {
        validate_identifier(site_id)?;
        self.delete_deploy_key(site_id).await?;
        let repository = self.repositories.join(format!("{site_id}.git"));
        if let Err(error) = tokio::fs::remove_dir_all(repository).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error).context("failed to remove repository cache");
        }
        Ok(())
    }
}

async fn fetch_repository(
    command_timeout: Duration,
    repository: &Path,
    deploy_key: Option<PathBuf>,
    known_hosts: &Path,
    token: Option<&str>,
) -> Result<()> {
    run_git_authenticated(
        command_timeout,
        [
            OsStr::new("--git-dir"),
            repository.as_os_str(),
            OsStr::new("fetch"),
            OsStr::new("--prune"),
            OsStr::new("--no-tags"),
            OsStr::new("origin"),
            OsStr::new("+refs/heads/*:refs/remotes/origin/*"),
        ],
        deploy_key.as_deref(),
        known_hosts,
        token,
    )
    .await?;
    Ok(())
}

async fn run_git_authenticated<I, S>(
    command_timeout: Duration,
    args: I,
    deploy_key: Option<&Path>,
    known_hosts: &Path,
    token: Option<&str>,
) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    if let Some(key) = deploy_key {
        command.env("GIT_SSH_COMMAND", format!("ssh -i {} -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile={}", shell_quote(key), shell_quote(known_hosts)));
    }
    if let Some(token) = token {
        use base64::Engine;
        let value =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: Basic {value}"),
            );
    }
    command_output(command_timeout, command).await
}

async fn run_git<I, S>(command_timeout: Duration, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command_output(command_timeout, command).await
}

async fn command_output(command_timeout: Duration, mut command: Command) -> Result<String> {
    let output = timeout(command_timeout, command.output())
        .await
        .context("Git command timed out")??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "Git command failed: {}",
            if stderr.is_empty() {
                output.status.to_string()
            } else {
                stderr
            }
        );
    }
    String::from_utf8(output.stdout).context("Git returned non-UTF-8 output")
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

pub(crate) fn validate_repository_url(url: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty()
        || url.starts_with('-')
        || url.starts_with("ext::")
        || url.contains(['\n', '\r', '\0'])
    {
        bail!("invalid repository URL");
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!("invalid internal identifier");
    }
    Ok(())
}

pub(crate) fn validate_branch(value: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.starts_with('/')
        || value.ends_with('/')
        || value.ends_with('.')
        || value.ends_with(".lock")
        || value.contains("//")
        || value.contains("..")
        || value.contains("@{")
        || value.contains(['~', '^', ':', '?', '*', '[', '\\', ' ', '\t', '\n'])
    {
        bail!("invalid branch name");
    }
    Ok(())
}

fn validate_tree_path(value: &str) -> Result<()> {
    if value.len() > 512
        || value.starts_with('/')
        || value.contains(['\\', '\0', '\n', '\r', ':'])
        || value.split('/').any(|part| part == "..")
    {
        bail!("invalid repository path");
    }
    Ok(())
}

#[allow(dead_code)]
fn validate_sha(value: &str) -> Result<()> {
    if !(7..=64).contains(&value.len()) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid commit SHA");
    }
    Ok(())
}

fn parse_ls_remote(output: &str) -> Result<RemoteInspection> {
    let mut default_branch = None;
    let mut branches = Vec::new();
    for line in output.lines() {
        if let Some(reference) = line
            .strip_prefix("ref: refs/heads/")
            .and_then(|line| line.split_once('\t').map(|pair| pair.0))
        {
            default_branch = Some(reference.to_owned());
            continue;
        }
        let Some((_, reference)) = line.split_once('\t') else {
            continue;
        };
        if let Some(branch) = reference.strip_prefix("refs/heads/") {
            branches.push(branch.to_owned());
        }
    }
    branches.sort();
    branches.dedup();
    Ok(RemoteInspection {
        default_branch,
        branches,
    })
}

fn parse_commit(output: &str) -> Result<CommitMetadata> {
    let fields: Vec<_> = output.trim_end().split('\0').collect();
    if fields.len() != 5 {
        bail!("unexpected commit metadata from Git");
    }
    Ok(CommitMetadata {
        sha: fields[0].into(),
        message: fields[1].into(),
        author_name: fields[2].into(),
        author_email: fields[3].into(),
        authored_at: fields[4].into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command as StdCommand};
    #[test]
    fn rejects_git_remote_helpers() {
        assert!(validate_repository_url("ext::sh -c nope").is_err());
    }
    #[test]
    fn rejects_unsafe_identifiers() {
        assert!(validate_identifier("../../site").is_err());
    }
    #[test]
    fn parses_remote_branches() {
        let parsed = parse_ls_remote(
            "ref: refs/heads/main\tHEAD\naaaa\trefs/heads/main\nbbbb\trefs/heads/dev\n",
        )
        .unwrap();
        assert_eq!(parsed.default_branch.as_deref(), Some("main"));
        assert_eq!(parsed.branches, ["dev", "main"]);
    }

    fn git(directory: &Path, args: &[&str]) {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(directory)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[actix_web::test]
    async fn caches_resolves_and_checks_out_a_local_repository() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        fs::create_dir(&source).unwrap();
        git(&source, &["init", "--initial-branch=main"]);
        fs::create_dir(source.join("frontend")).unwrap();
        fs::write(source.join("index.html"), "hello Blank").unwrap();
        fs::write(source.join("frontend/package.json"), "{}").unwrap();
        git(&source, &["add", "index.html", "frontend/package.json"]);
        git(
            &source,
            &[
                "-c",
                "user.name=Blank Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "Initial site",
            ],
        );

        let service = GitService::new(temporary.path());
        let source_url = source.to_str().unwrap();
        let inspection = service.inspect_remote(source_url).await.unwrap();
        assert_eq!(inspection.default_branch.as_deref(), Some("main"));
        assert_eq!(inspection.branches, ["main"]);

        service.fetch("site-1", source_url).await.unwrap();
        let root = service.list_tree("site-1", "main", "").await.unwrap();
        assert_eq!(root[0].kind, "tree");
        assert_eq!(root[0].path, "frontend");
        let frontend = service
            .list_tree("site-1", "main", "frontend")
            .await
            .unwrap();
        assert_eq!(frontend[0].path, "frontend/package.json");
        let commit = service.resolve_commit("site-1", "main").await.unwrap();
        assert_eq!(commit.message, "Initial site");
        let worktree = service
            .create_worktree("site-1", "deployment-1", &commit.sha)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(worktree.path().join("index.html")).unwrap(),
            "hello Blank"
        );
        let path = worktree.path().to_owned();
        worktree.remove().await.unwrap();
        assert!(!path.exists());

        let public_key = service.generate_deploy_key("site-1").await.unwrap();
        assert!(public_key.starts_with("ssh-ed25519 "));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(service.deploy_key_path("site-1"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        service.delete_deploy_key("site-1").await.unwrap();
        assert!(service.deploy_key("site-1").await.unwrap().is_none());
    }
}
