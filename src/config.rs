use anyhow::Context as _;
use proc_exit::WithCodeResultExt;

pub fn dump_config(output_path: &std::path::Path, config: &mut Config) -> proc_exit::ExitResult {
    let cwd = std::env::current_dir().with_code(proc_exit::Code::USAGE_ERR)?;
    let repo = git2::Repository::discover(&cwd).with_code(proc_exit::Code::USAGE_ERR)?;

    config.add_repo(&repo);
    let output = config.dump([&crate::blame::THEME as &dyn ReflectField]);

    if output_path == std::path::Path::new("-") {
        use std::io::Write;
        std::io::stdout().write_all(output.as_bytes())?;
    } else {
        std::fs::write(output_path, &output)?;
    }

    Ok(())
}

pub struct Config {
    system: Option<git2::Config>,
    repo: Option<git2::Config>,
    env: InMemoryConfig,
    cli: InMemoryConfig,
}

impl Config {
    pub fn system() -> Self {
        let system = git2::Config::open_default().ok();
        let repo = None;
        let env = InMemoryConfig::git_env();
        let cli = InMemoryConfig::git_cli();
        Self {
            system,
            repo,
            env,
            cli,
        }
    }

    pub fn add_repo(&mut self, repo: &git2::Repository) {
        let config_path = git_dir_config(repo);
        let repo = git2::Config::open(&config_path).ok();
        self.repo = repo;
    }

    pub fn get<F: Field>(&self, field: &F) -> F::Output {
        field.get_from(&self)
    }

    pub fn dump<'f>(&self, fields: impl IntoIterator<Item = &'f dyn ReflectField>) -> String {
        use std::fmt::Write;

        let mut output = String::new();

        let mut prior_section = "";
        for field in fields {
            let (section, name) = field
                .name()
                .split_once('.')
                .unwrap_or_else(|| panic!("field `{}` is missing a section", field.name()));
            if section != prior_section {
                let _ = writeln!(&mut output, "[{}]", section);
                prior_section = section;
            }
            let _ = writeln!(&mut output, "\t{} = {}", name, field.dump(self));
        }

        output
    }

    pub fn sources(&self) -> impl Iterator<Item = &dyn ConfigSource> {
        [
            Some(&self.cli).map(|c| c as &dyn ConfigSource),
            Some(&self.env).map(|c| c as &dyn ConfigSource),
            self.repo.as_ref().map(|c| c as &dyn ConfigSource),
            self.system.as_ref().map(|c| c as &dyn ConfigSource),
        ]
        .into_iter()
        .flatten()
    }
}

fn git_dir_config(repo: &git2::Repository) -> std::path::PathBuf {
    repo.path().join("config")
}

pub trait ConfigSource {
    fn name(&self) -> &str;

    fn get_bool(&self, name: &str) -> anyhow::Result<bool>;
    fn get_i32(&self, name: &str) -> anyhow::Result<i32>;
    fn get_i64(&self, name: &str) -> anyhow::Result<i64>;
    fn get_string(&self, name: &str) -> anyhow::Result<String>;
    fn get_path(&self, name: &str) -> anyhow::Result<std::path::PathBuf>;
}

impl ConfigSource for Config {
    fn name(&self) -> &str {
        "git"
    }

    fn get_bool(&self, name: &str) -> anyhow::Result<bool> {
        for config in self.sources() {
            if let Ok(v) = config.get_bool(name) {
                return Ok(v);
            }
        }
        // Fallback to the first error
        self.sources()
            .next()
            .expect("always a source")
            .get_bool(name)
    }
    fn get_i32(&self, name: &str) -> anyhow::Result<i32> {
        for config in self.sources() {
            if let Ok(v) = config.get_i32(name) {
                return Ok(v);
            }
        }
        // Fallback to the first error
        self.sources()
            .next()
            .expect("always a source")
            .get_i32(name)
    }
    fn get_i64(&self, name: &str) -> anyhow::Result<i64> {
        for config in self.sources() {
            if let Ok(v) = config.get_i64(name) {
                return Ok(v);
            }
        }
        // Fallback to the first error
        self.sources()
            .next()
            .expect("always a source")
            .get_i64(name)
    }
    fn get_string(&self, name: &str) -> anyhow::Result<String> {
        for config in self.sources() {
            if let Ok(v) = config.get_string(name) {
                return Ok(v);
            }
        }
        // Fallback to the first error
        self.sources()
            .next()
            .expect("always a source")
            .get_string(name)
    }
    fn get_path(&self, name: &str) -> anyhow::Result<std::path::PathBuf> {
        for config in self.sources() {
            if let Ok(v) = config.get_path(name) {
                return Ok(v);
            }
        }
        // Fallback to the first error
        self.sources()
            .next()
            .expect("always a source")
            .get_path(name)
    }
}

impl ConfigSource for git2::Config {
    fn name(&self) -> &str {
        "git"
    }

    fn get_bool(&self, name: &str) -> anyhow::Result<bool> {
        self.get_bool(name).map_err(|e| e.into())
    }
    fn get_i32(&self, name: &str) -> anyhow::Result<i32> {
        self.get_i32(name).map_err(|e| e.into())
    }
    fn get_i64(&self, name: &str) -> anyhow::Result<i64> {
        self.get_i64(name).map_err(|e| e.into())
    }
    fn get_string(&self, name: &str) -> anyhow::Result<String> {
        self.get_string(name).map_err(|e| e.into())
    }
    fn get_path(&self, name: &str) -> anyhow::Result<std::path::PathBuf> {
        self.get_path(name).map_err(|e| e.into())
    }
}

pub struct InMemoryConfig {
    name: String,
    values: std::collections::BTreeMap<String, Vec<String>>,
}

impl InMemoryConfig {
    pub fn git_env() -> Self {
        Self::from_env("git-config-env", git_config_env::ConfigEnv::new().iter())
    }

    pub fn git_cli() -> Self {
        Self::from_env(
            "git-cli",
            git_config_env::ConfigParameters::new()
                .iter()
                .map(|(k, v)| (k, v.unwrap_or_else(|| std::borrow::Cow::Borrowed("true")))),
        )
    }

    pub fn from_env(
        name: impl Into<String>,
        env: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        let name = name.into();
        let mut values = std::collections::BTreeMap::new();
        for (key, value) in env {
            values
                .entry(key.into())
                .or_insert_with(Vec::new)
                .push(value.into());
        }
        Self { name, values }
    }

    fn get_str(&self, name: &str) -> anyhow::Result<&str> {
        let value = self
            .values
            .get(name)
            .context("field is missing")?
            .last()
            .expect("always at least one element");
        Ok(value)
    }
}

impl Default for InMemoryConfig {
    fn default() -> Self {
        Self {
            name: "null".to_owned(),
            values: Default::default(),
        }
    }
}

impl ConfigSource for InMemoryConfig {
    fn name(&self) -> &str {
        &self.name
    }

    fn get_bool(&self, name: &str) -> anyhow::Result<bool> {
        let v = self.get_str(name).unwrap_or("true");
        v.parse::<bool>().map_err(|e| e.into())
    }
    fn get_i32(&self, name: &str) -> anyhow::Result<i32> {
        self.get_str(name)
            .and_then(|v| v.parse::<i32>().map_err(|e| e.into()))
    }
    fn get_i64(&self, name: &str) -> anyhow::Result<i64> {
        self.get_str(name)
            .and_then(|v| v.parse::<i64>().map_err(|e| e.into()))
    }
    fn get_string(&self, name: &str) -> anyhow::Result<String> {
        self.get_str(name).map(|v| v.to_owned())
    }
    fn get_path(&self, name: &str) -> anyhow::Result<std::path::PathBuf> {
        self.get_string(name).map(|v| v.into())
    }
}

pub trait FieldReader<T> {
    fn get_field(&self, name: &str) -> anyhow::Result<T>;
}

impl<C: ConfigSource> FieldReader<bool> for C {
    fn get_field(&self, name: &str) -> anyhow::Result<bool> {
        self.get_bool(name)
            .with_context(|| anyhow::format_err!("failed to read `{}`", name))
    }
}

impl<C: ConfigSource> FieldReader<i32> for C {
    fn get_field(&self, name: &str) -> anyhow::Result<i32> {
        self.get_i32(name)
            .with_context(|| anyhow::format_err!("failed to read `{}`", name))
    }
}

impl<C: ConfigSource> FieldReader<i64> for C {
    fn get_field(&self, name: &str) -> anyhow::Result<i64> {
        self.get_i64(name)
            .with_context(|| anyhow::format_err!("failed to read `{}`", name))
    }
}

impl<C: ConfigSource> FieldReader<String> for C {
    fn get_field(&self, name: &str) -> anyhow::Result<String> {
        self.get_string(name)
            .with_context(|| anyhow::format_err!("failed to read `{}`", name))
    }
}

impl<C: ConfigSource> FieldReader<std::path::PathBuf> for C {
    fn get_field(&self, name: &str) -> anyhow::Result<std::path::PathBuf> {
        self.get_path(name)
            .with_context(|| anyhow::format_err!("failed to read `{}`", name))
    }
}

pub trait Field {
    type Output;

    fn name(&self) -> &'static str;
    fn get_from(&self, config: &Config) -> Self::Output;
}

pub struct RawField<R> {
    name: &'static str,
    _type: std::marker::PhantomData<R>,
}

impl<R> RawField<R> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _type: std::marker::PhantomData,
        }
    }

    pub const fn fallback(self, fallback: FallbackFn<R>) -> FallbackField<R> {
        FallbackField {
            field: self,
            fallback,
        }
    }
}

impl<R> Field for RawField<R>
where
    Config: FieldReader<R>,
{
    type Output = Option<R>;

    fn name(&self) -> &'static str {
        self.name
    }

    fn get_from(&self, config: &Config) -> Self::Output {
        config.get_field(self.name).ok()
    }
}

type FallbackFn<R> = fn(&Config) -> R;

pub struct FallbackField<R> {
    field: RawField<R>,
    fallback: FallbackFn<R>,
}

impl<R> Field for FallbackField<R>
where
    Config: FieldReader<R>,
{
    type Output = R;

    fn name(&self) -> &'static str {
        self.field.name()
    }

    fn get_from(&self, config: &Config) -> Self::Output {
        self.field
            .get_from(config)
            .unwrap_or_else(|| (self.fallback)(config))
    }
}

pub trait ReflectField {
    fn name(&self) -> &'static str;

    fn dump(&self, config: &Config) -> String;
}

impl<F> ReflectField for F
where
    F: Field,
    F::Output: std::fmt::Display,
{
    fn name(&self) -> &'static str {
        self.name()
    }

    fn dump(&self, config: &Config) -> String {
        self.get_from(config).to_string()
    }
}
