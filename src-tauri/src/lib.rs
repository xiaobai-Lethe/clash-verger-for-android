use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::VecDeque,
    ffi::CStr,
    fs,
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};

type CmdResult<T = ()> = Result<T, String>;

const MIHOMO_BYTES: &[u8] =
    include_bytes!("../../../mihomo-android-arm64-v8-v1.19.24/mihomo-android-arm64-v8");
const DEFAULT_MIXED_PORT: u16 = 7897;
const DEFAULT_CONTROLLER_PORT: u16 = 9097;
const LOG_TAG: &str = "clash-verger-core";

#[derive(Clone)]
struct AppState {
    runtime: Arc<AndroidRuntime>,
}

struct AndroidRuntime {
    app_dir: PathBuf,
    core_dir: PathBuf,
    profile_dir: PathBuf,
    backup_dir: PathBuf,
    log_dir: PathBuf,
    config_path: PathBuf,
    verge_path: PathBuf,
    profiles_path: PathBuf,
    dns_path: PathBuf,
    process: Mutex<Option<Child>>,
    logs: Mutex<VecDeque<String>>,
    started_at: Instant,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct CoreStatus {
    running: bool,
    controller_port: u16,
    mixed_port: u16,
    core_path: String,
    runtime_path: String,
}

#[derive(Debug, Deserialize)]
struct MihomoHttpRequest {
    path: String,
    method: String,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
struct ValidationOutcome {
    status: &'static str,
}

struct IpLookupService {
    name: &'static str,
    url: &'static str,
}

fn stringify_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(target_os = "android")]
fn android_log(line: &str) {
    use std::ffi::CString;
    use std::os::raw::c_char;

    const ANDROID_LOG_INFO: i32 = 4;

    extern "C" {
        fn __android_log_write(prio: i32, tag: *const c_char, text: *const c_char) -> i32;
    }

    let tag = CString::new(LOG_TAG).ok();
    let text = CString::new(line).ok();
    if let (Some(tag), Some(text)) = (tag, text) {
        unsafe {
            __android_log_write(ANDROID_LOG_INFO, tag.as_ptr(), text.as_ptr());
        }
    }
}

#[cfg(not(target_os = "android"))]
fn android_log(line: &str) {
    println!("[{LOG_TAG}] {line}");
}

fn default_runtime_config() -> Value {
    json!({
        "allow-lan": true,
        "bind-address": "*",
        "mode": "rule",
        "log-level": "info",
        "ipv6": true,
        "external-controller": format!("127.0.0.1:{DEFAULT_CONTROLLER_PORT}"),
        "secret": "",
        "unified-delay": true,
        "find-process-mode": "off",
        "listeners": [
            {
                "name": "android-mixed",
                "type": "mixed",
                "port": DEFAULT_MIXED_PORT,
                "listen": "0.0.0.0",
                "udp": false
            }
        ],
        "tun": {
            "enable": false,
            "stack": "system",
            "device": "utun",
            "auto-route": false,
            "auto-detect-interface": false,
            "strict-route": false,
            "dns-hijack": ["any:53"],
            "mtu": 9000
        },
        "dns": {
            "enable": true,
            "listen": "127.0.0.1:1053",
            "enhanced-mode": "fake-ip",
            "nameserver": ["https://dns.google/dns-query", "https://cloudflare-dns.com/dns-query"]
        },
        "proxies": [],
        "proxy-groups": [
            { "name": "GLOBAL", "type": "select", "proxies": ["DIRECT"] }
        ],
        "rules": ["MATCH,DIRECT"]
    })
}

fn default_verge_config() -> Value {
    json!({
        "language": "zh",
        "theme_mode": "system",
        "enable_tun_mode": false,
        "enable_system_proxy": false,
        "proxy_auto_config": false,
        "proxy_host": "127.0.0.1",
        "verge_mixed_port": DEFAULT_MIXED_PORT,
        "auto_close_connection": false,
        "enable_clash_fields": true,
        "enable_builtin_enhanced": true,
        "startup_silent": false
    })
}

fn default_profiles_config() -> Value {
    json!({
        "current": null,
        "items": []
    })
}

impl AndroidRuntime {
    fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let base = app
            .path()
            .app_data_dir()
            .context("failed to resolve app data dir")?;
        let core_dir = base.join("core");
        let profile_dir = base.join("profiles");
        let backup_dir = base.join("backups");
        let log_dir = base.join("logs");
        fs::create_dir_all(&core_dir)?;
        fs::create_dir_all(&profile_dir)?;
        fs::create_dir_all(&backup_dir)?;
        fs::create_dir_all(&log_dir)?;

        let runtime = Self {
            config_path: base.join("runtime.yaml"),
            verge_path: base.join("verge.json"),
            profiles_path: base.join("profiles.json"),
            dns_path: base.join("dns.yaml"),
            app_dir: base,
            core_dir,
            profile_dir,
            backup_dir,
            log_dir,
            process: Mutex::new(None),
            logs: Mutex::new(VecDeque::with_capacity(1000)),
            started_at: Instant::now(),
            http: reqwest::Client::new(),
        };
        runtime.ensure_defaults()?;
        Ok(runtime)
    }

    fn ensure_defaults(&self) -> anyhow::Result<()> {
        self.write_json_default(&self.verge_path, &default_verge_config())?;
        self.write_json_default(&self.profiles_path, &default_profiles_config())?;
        if !self.config_path.exists() {
            let mut config = default_runtime_config();
            Self::apply_android_runtime_overrides(&mut config)?;
            let yaml = serde_yaml::to_string(&config)?;
            fs::write(&self.config_path, yaml)?;
        } else {
            let mut config = self.read_runtime_config()?;
            Self::apply_android_runtime_overrides(&mut config)?;
            self.write_runtime_config(&config)?;
        }
        if !self.dns_path.exists() {
            fs::write(&self.dns_path, "enable: true\n")?;
        }
        Ok(())
    }

    fn write_json_default(&self, path: &Path, value: &Value) -> anyhow::Result<()> {
        if !path.exists() {
            fs::write(path, serde_json::to_vec_pretty(value)?)?;
        }
        Ok(())
    }

    fn read_json(&self, path: &Path, fallback: Value) -> anyhow::Result<Value> {
        if !path.exists() {
            return Ok(fallback);
        }
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text).unwrap_or(fallback))
    }

    fn read_profiles_config(&self) -> anyhow::Result<Value> {
        let mut profiles = self.read_json(&self.profiles_path, default_profiles_config())?;
        self.normalize_profiles_config(&mut profiles)?;
        Ok(profiles)
    }

    fn normalize_profiles_config(&self, profiles: &mut Value) -> anyhow::Result<()> {
        if !profiles.is_object() {
            *profiles = default_profiles_config();
        }

        let obj = profiles
            .as_object_mut()
            .ok_or_else(|| anyhow!("invalid profiles config"))?;

        if !obj.get("items").is_some_and(Value::is_array) {
            obj.insert("items".to_string(), Value::Array(vec![]));
        }

        {
            let items = obj
                .get_mut("items")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| anyhow!("invalid profiles items"))?;

            let mut known_files: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    item.get("file")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .collect();

            if let Ok(entries) = fs::read_dir(&self.profile_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                        continue;
                    };
                    if !(file_name.ends_with(".yaml") || file_name.ends_with(".yml")) {
                        continue;
                    }
                    if known_files.iter().any(|known| known == file_name) {
                        continue;
                    }

                    let uid = file_name
                        .trim_end_matches(".yaml")
                        .trim_end_matches(".yml")
                        .to_string();
                    known_files.push(file_name.to_string());
                    items.push(json!({
                        "uid": uid,
                        "name": uid,
                        "type": "local",
                        "file": file_name
                    }));
                }
            }
        }

        if obj.get("current").and_then(Value::as_str).is_none() {
            let first_uid = obj
                .get("items")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("uid"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            if let Some(uid) = first_uid {
                obj.insert("current".to_string(), Value::String(uid.to_string()));
            } else {
                obj.insert("current".to_string(), Value::Null);
            }
        }

        Ok(())
    }

    fn write_json(&self, path: &Path, value: &Value) -> anyhow::Result<()> {
        fs::write(path, serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }

    fn read_runtime_config(&self) -> anyhow::Result<Value> {
        if !self.config_path.exists() {
            return Ok(default_runtime_config());
        }
        let text = fs::read_to_string(&self.config_path)?;
        Ok(serde_yaml::from_str(&text).unwrap_or_else(|_| default_runtime_config()))
    }

    fn write_runtime_config(&self, value: &Value) -> anyhow::Result<()> {
        fs::write(&self.config_path, serde_yaml::to_string(value)?)?;
        Ok(())
    }

    fn current_profile_file(&self) -> anyhow::Result<Option<(String, PathBuf)>> {
        let profiles = self.read_profiles_config()?;
        let current_uid = profiles
            .get("current")
            .and_then(Value::as_str)
            .filter(|uid| !uid.is_empty())
            .map(ToOwned::to_owned);

        let Some(current_uid) = current_uid else {
            return Ok(None);
        };

        let current_item = profiles
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("uid").and_then(Value::as_str) == Some(current_uid.as_str())
                })
            });

        let configured_file = current_item
            .and_then(|item| item.get("file"))
            .and_then(Value::as_str)
            .and_then(|file| Path::new(file).file_name())
            .and_then(|file| file.to_str())
            .filter(|file| !file.is_empty())
            .map(ToOwned::to_owned);

        let mut candidates = Vec::new();
        if let Some(file) = configured_file {
            candidates.push(self.profile_dir.join(file));
        }
        candidates.push(self.profile_dir.join(format!("{current_uid}.yaml")));
        candidates.push(self.profile_dir.join(format!("{current_uid}.yml")));

        let path = candidates
            .into_iter()
            .find(|path| path.exists())
            .ok_or_else(|| anyhow!("current profile file not found: {current_uid}"))?;

        Ok(Some((current_uid, path)))
    }

    fn apply_android_runtime_overrides(config: &mut Value) -> anyhow::Result<()> {
        if !config.is_object() {
            return Err(anyhow!("profile config must be a YAML object"));
        }

        let object = config
            .as_object_mut()
            .ok_or_else(|| anyhow!("profile config must be a YAML object"))?;

        object.remove("mixed-port");
        object.remove("socks-port");
        object.remove("port");
        object.remove("redir-port");
        object.remove("tproxy-port");
        object.insert("allow-lan".to_string(), Value::Bool(true));
        object.insert("bind-address".to_string(), Value::String("*".to_string()));
        object.insert(
            "find-process-mode".to_string(),
            Value::String("off".to_string()),
        );
        object.insert(
            "external-controller".to_string(),
            Value::String(format!("127.0.0.1:{DEFAULT_CONTROLLER_PORT}")),
        );
        object.insert("secret".to_string(), Value::String(String::new()));
        object.remove("interface-name");
        object.remove("routing-mark");
        object.insert(
            "listeners".to_string(),
            json!([
                {
                    "name": "android-mixed",
                    "type": "mixed",
                    "port": DEFAULT_MIXED_PORT,
                    "listen": "0.0.0.0",
                    "udp": false
                }
            ]),
        );

        let dns = object
            .entry("dns".to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow!("profile dns field must be a YAML object"))?;
        dns.insert("enable".to_string(), Value::Bool(true));
        dns.insert(
            "listen".to_string(),
            Value::String("127.0.0.1:1053".to_string()),
        );

        let tun = object
            .entry("tun".to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow!("profile tun field must be a YAML object"))?;
        tun.insert("enable".to_string(), Value::Bool(false));
        tun.insert("auto-route".to_string(), Value::Bool(false));
        tun.insert("auto-detect-interface".to_string(), Value::Bool(false));
        tun.insert("strict-route".to_string(), Value::Bool(false));

        Ok(())
    }

    fn write_runtime_from_current_profile(&self) -> anyhow::Result<Option<String>> {
        let Some((uid, path)) = self.current_profile_file()? else {
            return Ok(None);
        };

        let profile = fs::read_to_string(&path)
            .with_context(|| format!("failed to read profile file: {}", path.display()))?;
        let mut config: Value = serde_yaml::from_str(&profile)
            .with_context(|| format!("failed to parse profile YAML: {}", path.display()))?;

        Self::apply_android_runtime_overrides(&mut config)?;
        self.write_runtime_config(&config)?;
        Ok(Some(uid))
    }

    fn patch_object(target: &mut Value, patch: Value) {
        if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
            for (key, value) in patch {
                target.insert(key.clone(), value.clone());
            }
        }
    }

    fn core_path(&self) -> PathBuf {
        self.core_dir.join("mihomo")
    }

    fn bundled_native_core_path(&self) -> Option<PathBuf> {
        let maps = fs::read_to_string("/proc/self/maps").ok()?;
        for line in maps.lines() {
            if !line.contains("libclash_verger_for_android_lib.so") {
                continue;
            }

            let path = line.rsplit_once(' ').map(|(_, path)| path.trim())?;
            if path.contains("base.apk!") {
                continue;
            }

            let candidate = Path::new(path)
                .parent()
                .map(|dir| dir.join("libmihomo.so"))?;
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn ensure_core_binary(&self) -> anyhow::Result<PathBuf> {
        if let Some(path) = self.bundled_native_core_path() {
            return Ok(path);
        }

        let path = self.core_path();
        let needs_write = fs::metadata(&path)
            .map(|m| m.len() != MIHOMO_BYTES.len() as u64)
            .unwrap_or(true);
        if needs_write {
            let mut file = fs::File::create(&path)?;
            file.write_all(MIHOMO_BYTES)?;
        }
        set_executable(&path)?;
        Ok(path)
    }

    async fn append_log(&self, line: String) {
        android_log(&line);
        let mut logs = self.logs.lock().await;
        if logs.len() >= 1000 {
            logs.pop_front();
        }
        logs.push_back(line);
    }

    async fn append_runtime_log(&self, line: impl Into<String>) {
        self.append_log(line.into()).await;
    }

    async fn start_core(&self, app: AppHandle) -> anyhow::Result<()> {
        let mut child_guard = self.process.lock().await;
        if let Some(child) = child_guard.as_mut() {
            match child.try_wait()? {
                Some(status) => {
                    self.append_runtime_log(format!("previous mihomo process exited: {status}"))
                        .await;
                    *child_guard = None;
                }
                None => return Ok(()),
            }
        }

        let core = self.ensure_core_binary()?;
        self.append_runtime_log(format!("mihomo binary path: {}", core.display()))
            .await;

        let mut command = Command::new(core);
        command
            .arg("-d")
            .arg(&self.app_dir)
            .arg("-f")
            .arg(&self.config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        self.append_runtime_log(format!(
            "starting mihomo: -d {} -f {}",
            self.app_dir.display(),
            self.config_path.display()
        ))
        .await;

        let mut child = command.spawn().context("failed to start mihomo")?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Some(stdout) = stdout {
            let runtime = app.state::<AppState>().runtime.clone();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    runtime.append_log(format!("[mihomo:stdout] {line}")).await;
                }
            });
        }
        if let Some(stderr) = stderr {
            let runtime = app.state::<AppState>().runtime.clone();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    runtime.append_log(format!("[mihomo:stderr] {line}")).await;
                }
            });
        }
        self.append_runtime_log("mihomo process started").await;
        *child_guard = Some(child);
        Ok(())
    }

    async fn stop_core(&self) -> anyhow::Result<()> {
        let mut child_guard = self.process.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
            self.append_runtime_log("mihomo process stopped").await;
        }
        Ok(())
    }

    async fn core_status(&self) -> anyhow::Result<CoreStatus> {
        let mut child_guard = self.process.lock().await;
        let mut running = false;
        if let Some(child) = child_guard.as_mut() {
            match child.try_wait()? {
                Some(status) => {
                    self.append_runtime_log(format!("mihomo process exited: {status}"))
                        .await;
                    *child_guard = None;
                }
                None => running = true,
            }
        }

        let core_path = self
            .ensure_core_binary()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|error| format!("unavailable: {error}"));

        Ok(CoreStatus {
            running,
            controller_port: DEFAULT_CONTROLLER_PORT,
            mixed_port: DEFAULT_MIXED_PORT,
            core_path,
            runtime_path: self.config_path.to_string_lossy().to_string(),
        })
    }

    async fn controller_request(&self, request: MihomoHttpRequest) -> anyhow::Result<Value> {
        let path = if request.path.starts_with('/') {
            request.path
        } else {
            format!("/{}", request.path)
        };
        let url = format!("http://127.0.0.1:{DEFAULT_CONTROLLER_PORT}{path}");
        let method = request.method.parse().unwrap_or(reqwest::Method::GET);
        let mut builder = self.http.request(method, url);
        if let Some(body) = request.body {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let response = builder.send().await?;
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("mihomo controller returned {status}: {text}"));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    async fn lookup_ip_info(&self) -> anyhow::Result<Value> {
        const SERVICES: &[IpLookupService] = &[
            IpLookupService {
                name: "ip.sb",
                url: "https://api.ip.sb/geoip",
            },
            IpLookupService {
                name: "ipapi.co",
                url: "https://ipapi.co/json",
            },
            IpLookupService {
                name: "ipapi.is",
                url: "https://api.ipapi.is/",
            },
            IpLookupService {
                name: "ipwho.is",
                url: "https://ipwho.is/",
            },
            IpLookupService {
                name: "skk",
                url: "https://ip.api.skk.moe/cf-geoip",
            },
            IpLookupService {
                name: "geojs",
                url: "https://get.geojs.io/v1/ip/geo.json",
            },
        ];

        let mut last_error = String::new();
        for service in SERVICES {
            match self.lookup_ip_info_from(service).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    last_error = format!("{}: {error}", service.name);
                    self.append_runtime_log(format!("ip lookup failed via {last_error}"))
                        .await;
                }
            }
        }

        Err(anyhow!(
            "all IP lookup services failed{}",
            if last_error.is_empty() {
                String::new()
            } else {
                format!("; last error: {last_error}")
            }
        ))
    }

    async fn lookup_ip_info_from(&self, service: &IpLookupService) -> anyhow::Result<Value> {
        let response = self
            .http
            .get(service.url)
            .header(reqwest::header::USER_AGENT, "clash-verger-for-android")
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {status}: {text}"));
        }
        let data: Value = serde_json::from_str(&text)
            .with_context(|| format!("invalid JSON response from {}", service.name))?;
        let mut info = match service.name {
            "ip.sb" => map_ip_sb(&data),
            "ipapi.co" => map_ipapi_co(&data),
            "ipapi.is" => map_ipapi_is(&data),
            "ipwho.is" => map_ipwho_is(&data),
            "skk" => map_skk_ip(&data),
            "geojs" => map_geojs(&data),
            _ => Value::Null,
        };

        let ip = info.get("ip").and_then(Value::as_str).unwrap_or_default();
        if ip.is_empty() {
            return Err(anyhow!("missing ip field in {}", service.name));
        }

        if let Some(object) = info.as_object_mut() {
            object.insert(
                "lastFetchTs".into(),
                json!(chrono::Utc::now().timestamp_millis()),
            );
            object.insert("source".into(), json!(service.name));
        }
        Ok(info)
    }
}

fn value_string(data: &Value, keys: &[&str]) -> String {
    value_at(data, keys)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn value_number(data: &Value, keys: &[&str]) -> f64 {
    if let Some(value) = value_at(data, keys) {
        if let Some(number) = value.as_f64() {
            return number;
        }
        if let Some(text) = value.as_str() {
            return text.parse().unwrap_or_default();
        }
    }
    0.0
}

fn value_u64(data: &Value, keys: &[&str]) -> u64 {
    if let Some(value) = value_at(data, keys) {
        if let Some(number) = value.as_u64() {
            return number;
        }
        if let Some(text) = value.as_str() {
            return text.trim_start_matches("AS").parse().unwrap_or_default();
        }
    }
    0
}

fn value_at<'a>(data: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let mut current = data;
    for key in keys {
        current = current.get(*key)?;
    }
    Some(current)
}

fn ip_info_json(
    ip: String,
    country_code: String,
    country: String,
    region: String,
    city: String,
    organization: String,
    asn: u64,
    asn_organization: String,
    longitude: f64,
    latitude: f64,
    timezone: String,
) -> Value {
    json!({
        "ip": ip,
        "country_code": country_code,
        "country": country,
        "region": region,
        "city": city,
        "organization": organization,
        "asn": asn,
        "asn_organization": asn_organization,
        "longitude": longitude,
        "latitude": latitude,
        "timezone": timezone,
    })
}

fn map_ip_sb(data: &Value) -> Value {
    ip_info_json(
        value_string(data, &["ip"]),
        value_string(data, &["country_code"]),
        value_string(data, &["country"]),
        value_string(data, &["region"]),
        value_string(data, &["city"]),
        value_string(data, &["organization"]),
        value_u64(data, &["asn"]),
        value_string(data, &["asn_organization"]),
        value_number(data, &["longitude"]),
        value_number(data, &["latitude"]),
        value_string(data, &["timezone"]),
    )
}

fn map_ipapi_co(data: &Value) -> Value {
    let org = value_string(data, &["org"]);
    ip_info_json(
        value_string(data, &["ip"]),
        value_string(data, &["country_code"]),
        value_string(data, &["country_name"]),
        value_string(data, &["region"]),
        value_string(data, &["city"]),
        org.clone(),
        value_u64(data, &["asn"]),
        org,
        value_number(data, &["longitude"]),
        value_number(data, &["latitude"]),
        value_string(data, &["timezone"]),
    )
}

fn map_ipapi_is(data: &Value) -> Value {
    let org = value_string(data, &["asn", "org"]);
    ip_info_json(
        value_string(data, &["ip"]),
        value_string(data, &["location", "country_code"]),
        value_string(data, &["location", "country"]),
        value_string(data, &["location", "state"]),
        value_string(data, &["location", "city"]),
        if org.is_empty() {
            value_string(data, &["company", "name"])
        } else {
            org.clone()
        },
        value_u64(data, &["asn", "asn"]),
        org,
        value_number(data, &["location", "longitude"]),
        value_number(data, &["location", "latitude"]),
        value_string(data, &["location", "timezone"]),
    )
}

fn map_ipwho_is(data: &Value) -> Value {
    ip_info_json(
        value_string(data, &["ip"]),
        value_string(data, &["country_code"]),
        value_string(data, &["country"]),
        value_string(data, &["region"]),
        value_string(data, &["city"]),
        value_string(data, &["connection", "org"]),
        value_u64(data, &["connection", "asn"]),
        value_string(data, &["connection", "isp"]),
        value_number(data, &["longitude"]),
        value_number(data, &["latitude"]),
        value_string(data, &["timezone", "id"]),
    )
}

fn map_skk_ip(data: &Value) -> Value {
    let as_org = value_string(data, &["asOrg"]);
    ip_info_json(
        value_string(data, &["ip"]),
        value_string(data, &["country"]),
        value_string(data, &["country"]),
        value_string(data, &["region"]),
        value_string(data, &["city"]),
        as_org.clone(),
        value_u64(data, &["asn"]),
        as_org,
        value_number(data, &["longitude"]),
        value_number(data, &["latitude"]),
        value_string(data, &["timezone"]),
    )
}

fn map_geojs(data: &Value) -> Value {
    let org = value_string(data, &["organization_name"]);
    ip_info_json(
        value_string(data, &["ip"]),
        value_string(data, &["country_code"]),
        value_string(data, &["country"]),
        value_string(data, &["region"]),
        value_string(data, &["city"]),
        org.clone(),
        value_u64(data, &["asn"]),
        org,
        value_number(data, &["longitude"]),
        value_number(data, &["latitude"]),
        value_string(data, &["timezone"]),
    )
}

#[cfg(target_os = "android")]
fn android_network_interfaces_info() -> anyhow::Result<Vec<Value>> {
    let mut addrs: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut addrs) } != 0 {
        return Err(anyhow!("getifaddrs failed"));
    }

    let mut interfaces = Map::new();
    let mut cursor = addrs;
    while !cursor.is_null() {
        let ifa = unsafe { &*cursor };
        if !ifa.ifa_addr.is_null() && unsafe { (*ifa.ifa_addr).sa_family as i32 } == libc::AF_INET {
            let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
                .to_string_lossy()
                .to_string();
            let sockaddr = unsafe { *(ifa.ifa_addr as *const libc::sockaddr_in) };
            let ip = Ipv4Addr::from(u32::from_be(sockaddr.sin_addr.s_addr)).to_string();

            if is_android_lan_interface(&name) && is_private_ipv4(&ip) {
                let entry = interfaces.entry(name.clone()).or_insert_with(|| {
                    json!({
                        "name": name,
                        "addr": [],
                        "index": 0
                    })
                });
                if let Some(addr) = entry.get_mut("addr").and_then(Value::as_array_mut) {
                    addr.push(json!({ "V4": { "ip": ip } }));
                }
            }
        }
        cursor = ifa.ifa_next;
    }

    unsafe {
        libc::freeifaddrs(addrs);
    }

    Ok(interfaces.into_values().collect())
}

#[cfg(target_os = "android")]
fn is_android_lan_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if name.starts_with("tun")
        || name.starts_with("rmnet")
        || name.starts_with("ccmni")
        || name.starts_with("clat")
        || name.starts_with("dummy")
        || name.starts_with("lo")
        || name.starts_with("p2p")
    {
        return false;
    }

    name.starts_with("wlan")
        || name.starts_with("ap")
        || name.starts_with("swlan")
        || name.starts_with("eth")
        || name.starts_with("usb")
        || name.starts_with("rndis")
}

#[cfg(target_os = "android")]
fn is_private_ipv4(ip: &str) -> bool {
    let Ok(ip) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    let octets = ip.octets();
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

#[cfg(not(target_os = "android"))]
fn android_network_interfaces_info() -> anyhow::Result<Vec<Value>> {
    Ok(vec![])
}

#[cfg(unix)]
fn set_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn state(app: &AppHandle) -> Arc<AndroidRuntime> {
    app.state::<AppState>().runtime.clone()
}

fn validation_ok() -> ValidationOutcome {
    ValidationOutcome { status: "valid" }
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[tauri::command]
async fn get_app_dir(app: AppHandle) -> CmdResult<String> {
    Ok(state(&app).app_dir.to_string_lossy().to_string())
}

#[tauri::command]
async fn get_verge_config(app: AppHandle) -> CmdResult<Value> {
    let runtime = state(&app);
    runtime
        .read_json(&runtime.verge_path, default_verge_config())
        .map_err(stringify_error)
}

#[tauri::command]
async fn patch_verge_config(app: AppHandle, payload: Value) -> CmdResult {
    let runtime = state(&app);
    let mut value = runtime
        .read_json(&runtime.verge_path, default_verge_config())
        .map_err(stringify_error)?;
    AndroidRuntime::patch_object(&mut value, payload);
    runtime
        .write_json(&runtime.verge_path, &value)
        .map_err(stringify_error)
}

#[tauri::command]
async fn get_profiles(app: AppHandle) -> CmdResult<Value> {
    let runtime = state(&app);
    runtime.read_profiles_config().map_err(stringify_error)
}

#[tauri::command]
async fn patch_profiles_config(app: AppHandle, profiles: Value) -> CmdResult<ValidationOutcome> {
    let runtime = state(&app);
    let mut next = runtime.read_profiles_config().map_err(stringify_error)?;
    AndroidRuntime::patch_object(&mut next, profiles);
    runtime
        .normalize_profiles_config(&mut next)
        .map_err(stringify_error)?;
    runtime
        .write_json(&runtime.profiles_path, &next)
        .map_err(stringify_error)?;
    Ok(validation_ok())
}

#[tauri::command]
async fn enhance_profiles(app: AppHandle) -> CmdResult<ValidationOutcome> {
    let runtime = state(&app);
    let current_uid = runtime
        .write_runtime_from_current_profile()
        .map_err(stringify_error)?;

    if let Some(uid) = current_uid {
        runtime
            .append_runtime_log(format!("runtime config generated from profile: {uid}"))
            .await;
        runtime.stop_core().await.map_err(stringify_error)?;
        runtime
            .start_core(app.clone())
            .await
            .map_err(stringify_error)?;
        let _ = app.emit("profile-changed", uid);
        let _ = app.emit("verge://refresh-clash-config", ());
        let _ = app.emit("verge://refresh-proxy-config", ());
    } else {
        runtime
            .append_runtime_log("no current profile; runtime config unchanged")
            .await;
    }

    Ok(validation_ok())
}

#[tauri::command]
async fn get_runtime_config(app: AppHandle) -> CmdResult<Value> {
    state(&app).read_runtime_config().map_err(stringify_error)
}

#[tauri::command]
async fn get_runtime_yaml(app: AppHandle) -> CmdResult<Option<String>> {
    let runtime = state(&app);
    Ok(fs::read_to_string(&runtime.config_path).ok())
}

#[tauri::command]
async fn get_runtime_exists(app: AppHandle) -> CmdResult<Vec<String>> {
    let runtime = state(&app);
    Ok(vec![runtime.config_path.to_string_lossy().to_string()])
}

#[tauri::command]
async fn get_runtime_logs(app: AppHandle) -> CmdResult<Value> {
    let runtime = state(&app);
    let logs = runtime.logs.lock().await;
    let lines: Vec<Value> = logs.iter().map(|line| json!(["", line])).collect();
    Ok(json!({ "mihomo": lines }))
}

#[tauri::command]
async fn get_clash_logs(app: AppHandle) -> CmdResult<Vec<String>> {
    Ok(state(&app).logs.lock().await.iter().cloned().collect())
}

#[tauri::command]
async fn clear_logs(app: AppHandle) -> CmdResult {
    state(&app).logs.lock().await.clear();
    Ok(())
}

#[tauri::command]
async fn get_clash_info(app: AppHandle) -> CmdResult<Value> {
    let config = state(&app).read_runtime_config().map_err(stringify_error)?;
    Ok(json!({
        "mixed_port": DEFAULT_MIXED_PORT,
        "socks_port": config.get("socks-port").and_then(Value::as_u64),
        "redir_port": config.get("redir-port").and_then(Value::as_u64),
        "tproxy_port": config.get("tproxy-port").and_then(Value::as_u64),
        "server": "127.0.0.1"
    }))
}

#[tauri::command]
async fn patch_clash_config(app: AppHandle, payload: Value) -> CmdResult {
    let runtime = state(&app);
    let mut config = runtime.read_runtime_config().map_err(stringify_error)?;
    let mut normalized = Map::new();
    if let Some(map) = payload.as_object() {
        for (key, value) in map {
            normalized.insert(key.clone(), value.clone());
            let normalized_key = key.replace('_', "-");
            if normalized_key != *key {
                normalized.insert(normalized_key, value.clone());
            }
        }
    }
    AndroidRuntime::patch_object(&mut config, Value::Object(normalized));
    AndroidRuntime::apply_android_runtime_overrides(&mut config).map_err(stringify_error)?;
    runtime
        .write_runtime_config(&config)
        .map_err(stringify_error)
}

#[tauri::command]
async fn patch_clash_mode(app: AppHandle, payload: String) -> CmdResult {
    patch_clash_config(app, json!({ "mode": payload })).await
}

#[tauri::command]
async fn start_core(app: AppHandle) -> CmdResult {
    let runtime = state(&app);
    runtime
        .start_core(app.clone())
        .await
        .map_err(stringify_error)?;
    let _ = app.emit("verge://refresh-clash-config", ());
    Ok(())
}

#[tauri::command]
async fn stop_core(app: AppHandle) -> CmdResult {
    state(&app).stop_core().await.map_err(stringify_error)
}

#[tauri::command]
async fn get_core_status(app: AppHandle) -> CmdResult<CoreStatus> {
    state(&app).core_status().await.map_err(stringify_error)
}

#[tauri::command]
async fn restart_core(app: AppHandle) -> CmdResult {
    stop_core(app.clone()).await?;
    start_core(app).await
}

#[tauri::command]
async fn change_clash_core(_clash_core: String) -> CmdResult<Option<String>> {
    Ok(Some("mihomo-android-arm64-v8-v1.19.24".into()))
}

#[tauri::command]
async fn mihomo_version(app: AppHandle) -> CmdResult<Value> {
    let runtime = state(&app);
    let core = runtime.ensure_core_binary().map_err(stringify_error)?;
    let output = Command::new(core)
        .arg("-v")
        .output()
        .await
        .map_err(stringify_error)?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if text.is_empty() {
        "mihomo android".to_string()
    } else {
        text
    };
    Ok(json!({ "version": version, "meta": true }))
}

#[tauri::command]
async fn mihomo_http(
    app: AppHandle,
    path: String,
    method: String,
    body: Option<String>,
) -> CmdResult<Value> {
    state(&app)
        .controller_request(MihomoHttpRequest { path, method, body })
        .await
        .map_err(stringify_error)
}

#[tauri::command]
async fn mihomo_ws_url(path: String) -> CmdResult<String> {
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    Ok(format!("ws://127.0.0.1:{DEFAULT_CONTROLLER_PORT}{path}"))
}

#[tauri::command]
async fn clash_api_get_proxy_delay(
    app: AppHandle,
    name: String,
    url: String,
    timeout: u64,
) -> CmdResult<Value> {
    let path = format!(
        "/proxies/{}/delay?timeout={timeout}&url={}",
        urlencoding::encode(&name),
        urlencoding::encode(&url)
    );
    mihomo_http(app, path, "GET".into(), None).await
}

#[tauri::command]
async fn test_delay(url: String) -> CmdResult<u64> {
    let started = Instant::now();
    let result = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(stringify_error)?
        .get(url)
        .send()
        .await;

    match result {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            Ok(started.elapsed().as_millis() as u64)
        }
        Ok(_) | Err(_) => Ok(1_000_001),
    }
}

#[tauri::command]
async fn get_ip_info(app: AppHandle) -> CmdResult<Value> {
    state(&app).lookup_ip_info().await.map_err(stringify_error)
}

#[tauri::command]
async fn copy_clash_env() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn create_profile(app: AppHandle, item: Value, file_data: Option<String>) -> CmdResult {
    let runtime = state(&app);
    let mut profiles = runtime.read_profiles_config().map_err(stringify_error)?;
    let items = profiles
        .as_object_mut()
        .and_then(|obj| obj.get_mut("items"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid profiles config".to_string())?;
    let uid = item
        .get("uid")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("profile-{}", chrono::Utc::now().timestamp_millis()));
    if let Some(data) = file_data {
        fs::write(runtime.profile_dir.join(format!("{uid}.yaml")), data)
            .map_err(stringify_error)?;
    }
    let mut next = item.as_object().cloned().unwrap_or_default();
    next.entry("uid").or_insert(Value::String(uid.clone()));
    next.entry("name").or_insert(Value::String(uid.clone()));
    next.entry("file")
        .or_insert(Value::String(format!("{uid}.yaml")));
    items.push(Value::Object(next));
    if profiles.get("current").and_then(Value::as_str).is_none() {
        profiles["current"] = Value::String(uid);
    }
    runtime
        .write_json(&runtime.profiles_path, &profiles)
        .map_err(stringify_error)
}

#[tauri::command]
async fn read_profile_file(app: AppHandle, index: String) -> CmdResult<String> {
    let runtime = state(&app);
    fs::read_to_string(runtime.profile_dir.join(format!("{index}.yaml"))).map_err(stringify_error)
}

#[tauri::command]
async fn save_profile_file(
    app: AppHandle,
    index: String,
    file_data: String,
) -> CmdResult<ValidationOutcome> {
    let runtime = state(&app);
    fs::write(runtime.profile_dir.join(format!("{index}.yaml")), file_data)
        .map_err(stringify_error)?;
    Ok(validation_ok())
}

#[tauri::command]
async fn view_profile(_index: String) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn import_profile(app: AppHandle, url: String, option: Option<Value>) -> CmdResult {
    let data = if url.starts_with("http://") || url.starts_with("https://") {
        reqwest::get(&url)
            .await
            .map_err(stringify_error)?
            .text()
            .await
            .map_err(stringify_error)?
    } else {
        fs::read_to_string(&url).map_err(stringify_error)?
    };
    create_profile(
        app,
        json!({ "name": url, "type": "remote", "url": url, "option": option.unwrap_or(Value::Null) }),
        Some(data),
    ).await
}

#[tauri::command]
async fn update_profile(_index: String, _option: Option<Value>) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn delete_profile(app: AppHandle, index: String) -> CmdResult {
    let runtime = state(&app);
    let mut profiles = runtime.read_profiles_config().map_err(stringify_error)?;
    if let Some(items) = profiles.get_mut("items").and_then(Value::as_array_mut) {
        items.retain(|item| item.get("uid").and_then(Value::as_str) != Some(index.as_str()));
    }
    runtime
        .write_json(&runtime.profiles_path, &profiles)
        .map_err(stringify_error)?;
    let _ = fs::remove_file(runtime.profile_dir.join(format!("{index}.yaml")));
    Ok(())
}

#[tauri::command]
async fn patch_profile(app: AppHandle, index: String, profile: Value) -> CmdResult {
    let runtime = state(&app);
    let mut profiles = runtime.read_profiles_config().map_err(stringify_error)?;
    if let Some(items) = profiles.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            if item.get("uid").and_then(Value::as_str) == Some(index.as_str()) {
                AndroidRuntime::patch_object(item, profile);
                break;
            }
        }
    }
    runtime
        .write_json(&runtime.profiles_path, &profiles)
        .map_err(stringify_error)
}

#[tauri::command]
async fn reorder_profile(_active_id: String, _over_id: String) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn get_runtime_proxy_chain_config(_proxy_chain_exit_node: String) -> CmdResult<String> {
    Ok(String::new())
}

#[tauri::command]
async fn update_proxy_chain_config_in_runtime(_proxy_chain_config: Value) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn sync_tray_proxy_selection() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn get_sys_proxy() -> CmdResult<Value> {
    Ok(json!({ "enable": false, "server": "-", "bypass": "Android VpnService path" }))
}

#[tauri::command]
async fn get_auto_proxy() -> CmdResult<Value> {
    Ok(json!({ "enable": false, "url": "" }))
}

#[tauri::command]
async fn get_auto_launch_status() -> CmdResult<bool> {
    Ok(false)
}

#[tauri::command]
async fn restart_app() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn open_app_dir() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn open_core_dir() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn open_logs_dir() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn open_web_url(url: String) -> CmdResult {
    open::that(url).map_err(stringify_error)
}

#[tauri::command]
async fn invoke_uwp_tool() -> CmdResult {
    Err("Android does not support Windows UWP loopback tools".into())
}

#[tauri::command]
async fn get_portable_flag() -> CmdResult<bool> {
    Ok(false)
}

#[tauri::command]
async fn open_devtools() -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn exit_app(app: AppHandle) -> CmdResult {
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn export_diagnostic_info(app: AppHandle) -> CmdResult<Value> {
    Ok(json!({
        "appDir": state(&app).app_dir,
        "platform": "android",
        "core": "mihomo-android-arm64-v8-v1.19.24"
    }))
}

#[tauri::command]
async fn get_system_info() -> CmdResult<String> {
    Ok("Name: Android\nVersion: Android\nArch: arm64-v8a".into())
}

#[tauri::command]
async fn copy_icon_file(_path: String, _icon_info: Value) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn download_icon_cache(_url: String, name: String) -> CmdResult<String> {
    Ok(name)
}

#[tauri::command]
async fn get_network_interfaces() -> CmdResult<Vec<String>> {
    let interfaces = android_network_interfaces_info().map_err(stringify_error)?;
    Ok(interfaces
        .iter()
        .filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_string))
        .collect())
}

#[tauri::command]
async fn get_system_hostname() -> CmdResult<String> {
    Ok("android".into())
}

#[tauri::command]
async fn get_network_interfaces_info() -> CmdResult<Vec<Value>> {
    android_network_interfaces_info().map_err(stringify_error)
}

#[tauri::command]
async fn create_webdav_backup() -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn create_local_backup() -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn delete_webdav_backup(_filename: String) -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn delete_local_backup(_filename: String) -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn restore_webdav_backup(_filename: String) -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn restore_local_backup(_filename: String) -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn import_local_backup(_source: String) -> CmdResult<String> {
    Ok(String::new())
}
#[tauri::command]
async fn export_local_backup(_filename: String, _destination: String) -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn save_webdav_config(_url: String, _username: String, _password: String) -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn list_webdav_backup() -> CmdResult<Vec<Value>> {
    Ok(vec![])
}
#[tauri::command]
async fn list_local_backup() -> CmdResult<Vec<Value>> {
    Ok(vec![])
}

#[tauri::command]
async fn script_validate_notice(_status: String, _msg: String) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn validate_script_file(_file_path: String) -> CmdResult<ValidationOutcome> {
    Ok(validation_ok())
}

#[tauri::command]
async fn get_running_mode() -> CmdResult<String> {
    Ok("android-vpn".into())
}

#[tauri::command]
async fn get_app_uptime(app: AppHandle) -> CmdResult<u64> {
    Ok(state(&app).started_at.elapsed().as_secs())
}

#[tauri::command]
async fn install_service() -> CmdResult {
    Err("Android does not support desktop service installation".into())
}
#[tauri::command]
async fn uninstall_service() -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn reinstall_service() -> CmdResult {
    Err("Android does not support desktop service installation".into())
}
#[tauri::command]
async fn repair_service() -> CmdResult {
    Err("Android does not support desktop service repair".into())
}
#[tauri::command]
async fn is_service_available() -> CmdResult<bool> {
    Ok(false)
}
#[tauri::command]
async fn entry_lightweight_mode() -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn exit_lightweight_mode() -> CmdResult {
    Ok(())
}
#[tauri::command]
async fn app_is_admin() -> CmdResult<bool> {
    Ok(false)
}
#[tauri::command]
async fn get_next_update_time(_uid: String) -> CmdResult<Option<u64>> {
    Ok(None)
}
#[tauri::command]
async fn is_port_in_use(port: u16) -> CmdResult<bool> {
    Ok(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok())
}

#[tauri::command]
async fn get_unlock_items() -> CmdResult<Vec<Value>> {
    Ok(vec![])
}

#[tauri::command]
async fn check_media_unlock() -> CmdResult<Vec<Value>> {
    Ok(vec![])
}

#[tauri::command]
async fn get_dns_config_content(app: AppHandle) -> CmdResult<String> {
    let runtime = state(&app);
    fs::read_to_string(&runtime.dns_path).map_err(stringify_error)
}

#[tauri::command]
async fn save_dns_config(app: AppHandle, dns_config: String) -> CmdResult {
    let runtime = state(&app);
    fs::write(&runtime.dns_path, dns_config).map_err(stringify_error)
}

#[tauri::command]
async fn validate_config_file(_file_path: String) -> CmdResult<ValidationOutcome> {
    Ok(validation_ok())
}

#[tauri::command]
async fn apply_dns_config(_apply: bool) -> CmdResult {
    Ok(())
}

#[tauri::command]
async fn get_dns_config_exists(app: AppHandle) -> CmdResult<bool> {
    Ok(state(&app).dns_path.exists())
}

#[tauri::command]
async fn upgrade_core() -> CmdResult {
    Err("Core self-upgrade is disabled on Android; the bundled mihomo binary is used".into())
}

#[tauri::command]
async fn request_vpn_permission() -> CmdResult<Value> {
    Ok(
        json!({ "granted": true, "message": "VPN permission is requested by Android on app startup" }),
    )
}

#[tauri::command]
async fn start_vpn(app: AppHandle) -> CmdResult<Value> {
    start_core(app).await?;
    Ok(json!({
        "running": true,
        "message": "mihomo started; Android VpnService forwards traffic through 127.0.0.1:7897"
    }))
}

#[tauri::command]
async fn stop_vpn(app: AppHandle) -> CmdResult<Value> {
    stop_core(app).await?;
    Ok(json!({ "running": false }))
}

#[tauri::command]
async fn get_vpn_status(app: AppHandle) -> CmdResult<Value> {
    let status = state(&app).core_status().await.map_err(stringify_error)?;
    Ok(json!({
        "running": status.running,
        "permission": "unknown",
        "core_path": status.core_path,
        "controller_port": status.controller_port,
        "mixed_port": status.mixed_port
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let runtime = AndroidRuntime::new(app.handle())?;
            let runtime = Arc::new(runtime);
            app.manage(AppState {
                runtime: runtime.clone(),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match runtime.write_runtime_from_current_profile() {
                    Ok(Some(uid)) => {
                        runtime
                            .append_runtime_log(format!(
                                "runtime config generated from profile at startup: {uid}"
                            ))
                            .await;
                    }
                    Ok(None) => {
                        runtime
                            .append_runtime_log(
                                "no current profile at startup; using existing runtime config",
                            )
                            .await;
                    }
                    Err(error) => {
                        runtime
                            .append_runtime_log(format!(
                                "failed to generate runtime config at startup: {error}"
                            ))
                            .await;
                    }
                }
                if let Err(error) = runtime.start_core(handle).await {
                    runtime
                        .append_runtime_log(format!("failed to auto-start mihomo: {error}"))
                        .await;
                }
            });
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_app_dir,
            get_verge_config,
            patch_verge_config,
            get_profiles,
            patch_profiles_config,
            enhance_profiles,
            get_runtime_config,
            get_runtime_yaml,
            get_runtime_exists,
            get_runtime_logs,
            get_clash_logs,
            clear_logs,
            get_clash_info,
            patch_clash_config,
            patch_clash_mode,
            start_core,
            stop_core,
            get_core_status,
            restart_core,
            change_clash_core,
            mihomo_version,
            mihomo_http,
            mihomo_ws_url,
            clash_api_get_proxy_delay,
            test_delay,
            get_ip_info,
            copy_clash_env,
            create_profile,
            read_profile_file,
            save_profile_file,
            view_profile,
            import_profile,
            update_profile,
            delete_profile,
            patch_profile,
            reorder_profile,
            get_runtime_proxy_chain_config,
            update_proxy_chain_config_in_runtime,
            sync_tray_proxy_selection,
            get_sys_proxy,
            get_auto_proxy,
            get_auto_launch_status,
            restart_app,
            open_app_dir,
            open_core_dir,
            open_logs_dir,
            open_web_url,
            invoke_uwp_tool,
            get_portable_flag,
            open_devtools,
            exit_app,
            export_diagnostic_info,
            get_system_info,
            copy_icon_file,
            download_icon_cache,
            get_network_interfaces,
            get_system_hostname,
            get_network_interfaces_info,
            create_webdav_backup,
            create_local_backup,
            delete_webdav_backup,
            delete_local_backup,
            restore_webdav_backup,
            restore_local_backup,
            import_local_backup,
            export_local_backup,
            save_webdav_config,
            list_webdav_backup,
            list_local_backup,
            script_validate_notice,
            validate_script_file,
            get_running_mode,
            get_app_uptime,
            install_service,
            uninstall_service,
            reinstall_service,
            repair_service,
            is_service_available,
            entry_lightweight_mode,
            exit_lightweight_mode,
            app_is_admin,
            get_next_update_time,
            is_port_in_use,
            get_unlock_items,
            check_media_unlock,
            get_dns_config_content,
            save_dns_config,
            validate_config_file,
            apply_dns_config,
            get_dns_config_exists,
            upgrade_core,
            request_vpn_permission,
            start_vpn,
            stop_vpn,
            get_vpn_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
