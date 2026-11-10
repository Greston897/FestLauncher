use axum::{extract::State, http::{header, HeaderMap, StatusCode}, routing::{get, post}, Json, Router};
use mc_launcher_core::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{fs, path::{Path, PathBuf}, process::Command, sync::{Arc, Mutex}};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings { username: String, version: String, memory: u32, java: String, mods: Vec<String> }
impl Default for Settings { fn default() -> Self { Self { username: "Player".into(), version: "26.2".into(), memory: 4, java: "java".into(), mods: vec!["fabric-api".into(), "sodium".into(), "iris".into()] } } }
#[derive(Clone, Serialize)]
struct Status { phase: String, message: String, busy: bool }
#[derive(Clone)]
struct App { root: PathBuf, status: Arc<Mutex<Status>> }
fn status(app: &App, phase: &str, message: impl Into<String>, busy: bool) { *app.status.lock().unwrap() = Status { phase: phase.into(), message: message.into(), busy }; }
fn settings_path(app: &App) -> PathBuf { app.root.join("settings.json") }
fn load(app: &App) -> Settings { fs::read(settings_path(app)).ok().and_then(|s| serde_json::from_slice(&s).ok()).unwrap_or_default() }
fn validate(s: &Settings) -> Result<(), String> {
    if s.username.len() < 3 || s.username.len() > 16 || !s.username.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { return Err("Ник: от 3 до 16 латинских букв, цифр или _.".into()); }
    if s.version.is_empty() || s.version.len() > 32 || !s.version.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_') { return Err("Некорректная версия.".into()); }
    if !(2..=32).contains(&s.memory) { return Err("ОЗУ должно быть от 2 до 32 ГБ.".into()); }
    if s.java.trim().is_empty() { return Err("Укажите путь к Java.".into()); }
    if s.mods.iter().any(|m| !["fabric-api", "sodium", "iris", "visuality"].contains(&m.as_str())) { return Err("Неизвестный мод.".into()); }
    if s.mods.contains(&"iris".into()) && !s.mods.contains(&"sodium".into()) { return Err("Iris требует Sodium.".into()); }
    Ok(())
}
fn same_origin(headers: &HeaderMap) -> bool {
    // Mutating requests are only accepted from the locally served UI.
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    let origin = headers.get("origin").and_then(|h| h.to_str().ok()).unwrap_or("");
    origin == format!("http://{host}") && (host.starts_with("127.0.0.1:") || host.starts_with("localhost:"))
}
async fn get_settings(State(app): State<App>) -> Json<Settings> { Json(load(&app)) }
async fn get_status(State(app): State<App>) -> Json<Status> { Json(app.status.lock().unwrap().clone()) }
async fn save(State(app): State<App>, headers: HeaderMap, Json(s): Json<Settings>) -> Result<Json<Settings>, (StatusCode, String)> {
    if !same_origin(&headers) { return Err((StatusCode::FORBIDDEN, "Недопустимый источник запроса".into())); }
    validate(&s).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    fs::create_dir_all(&app.root).and_then(|_| fs::write(settings_path(&app), serde_json::to_vec_pretty(&s).unwrap())).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(s))
}
async fn versions() -> Result<Json<Value>, (StatusCode, String)> {
    let manifest: Value = reqwest::get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    let list: Vec<Value> = manifest["versions"].as_array().into_iter().flatten().filter(|v| v["type"] == "release").take(40).map(|v| v["id"].clone()).collect();
    Ok(Json(json!(list)))
}
fn err(e: impl std::fmt::Display) -> (StatusCode, String) { (StatusCode::BAD_GATEWAY, e.to_string()) }
async fn launch(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>, (StatusCode, String)> {
    if !same_origin(&headers) { return Err((StatusCode::FORBIDDEN, "Недопустимый источник запроса".into())); }
    let settings = load(&app); validate(&settings).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    { let mut st = app.status.lock().unwrap(); if st.busy { return Err((StatusCode::CONFLICT, "Игра уже запускается".into())); } *st = Status { phase: "installing".into(), message: "Подготовка Minecraft и Fabric…".into(), busy: true }; }
    std::thread::spawn(move || { if let Err(e) = run(&app, settings) { status(&app, "error", e, false); } });
    Ok(Json(json!({"ok":true})))
}
fn run(app: &App, s: Settings) -> Result<(), String> {
    let root = app.root.join("minecraft");
    let game = root.join("instances").join(&s.version);
    fs::create_dir_all(game.join("mods")).map_err(|e| e.to_string())?;
    let launcher = Launcher::new(&root);
    let installed = launcher.install(InstallRequest { minecraft_version: s.version.clone(), loader: Some(LoaderSpec::Fabric { version: LoaderVersion::LatestStable }), java: JavaInstallPolicy::Never }).map_err(|e| format!("Не удалось установить Minecraft/Fabric: {e}"))?;
    status(app, "installing", "Загрузка совместимых модов…", true);
    install_mods(&game, &s)?;
    status(app, "launching", "Запуск Minecraft…", true);
    let version = launcher.load_version(&installed.version_id).map_err(|e| e.to_string())?;
    let command = launcher.build_launch_command_from_version(&version, LaunchOptions { account: Account::offline(&s.username), java_executable: Some(PathBuf::from(&s.java)), game_directory: Some(game), launcher_name: "FestVisuals".into(), ..Default::default() }).map_err(|e| e.to_string())?;
    let mut child = Command::new(&command.executable).arg(format!("-Xmx{}G", s.memory)).args(&command.args).envs(command.env).current_dir(command.working_dir).spawn().map_err(|e| format!("Не удалось запустить Java ({}): {e}", s.java))?;
    status(app, "running", "Minecraft запущен", true);
    let exit = child.wait().map_err(|e| e.to_string())?;
    if exit.success() { status(app, "idle", "Игра закрыта", false); Ok(()) } else { Err(format!("Minecraft завершился с ошибкой: {exit}. Проверьте Java и latest.log в папке игры.")) }
}
fn install_mods(game: &Path, s: &Settings) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder().user_agent("FestVisuals/0.1 (Minecraft launcher)").build().map_err(|e| e.to_string())?;
    let mods_dir = game.join("mods");
    let manifest_path = game.join("fest-managed-mods.json");
    let previous: Vec<String> = fs::read(&manifest_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut downloads = Vec::new();
    for slug in &s.mods {
        let url = format!("https://api.modrinth.com/v2/project/{slug}/version");
        let versions: Value = client.get(url).query(&[("loaders", "[\"fabric\"]".to_string()), ("game_versions", format!("[\"{}\"]", s.version))]).send().and_then(|r| r.error_for_status()).and_then(|r| r.json()).map_err(|e| format!("Modrinth ({slug}): {e}"))?;
        let version = versions.as_array().and_then(|v| v.iter().find(|x| x["version_type"] == "release").or_else(|| v.first())).ok_or_else(|| format!("Для {slug} нет версии под Fabric {}. Отключите мод или выберите другую версию.", s.version))?;
        let file = version["files"].as_array().and_then(|a| a.iter().find(|f| f["primary"] == true).or_else(|| a.first())).ok_or_else(|| format!("Нет файла мода {slug}"))?;
        let url = file["url"].as_str().ok_or("Нет ссылки на мод")?;
        let expected = file["hashes"]["sha1"].as_str().ok_or("Нет контрольной суммы мода")?;
        let name = file["filename"].as_str().ok_or("Нет имени файла мода")?;
        if !url.starts_with("https://cdn.modrinth.com/") || Path::new(name).file_name().and_then(|x| x.to_str()) != Some(name) || !name.ends_with(".jar") { return Err("Небезопасный адрес или имя файла мода".into()); }
        let bytes = client.get(url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha1::digest(&bytes)) != expected { return Err(format!("Неверная контрольная сумма: {name}")); }
        if mods_dir.join(name).exists() && !previous.iter().any(|old| old == name) { return Err(format!("Файл {name} уже существует и не управляется лаунчером. Переименуйте или удалите его вручную.")); }
        downloads.push((name.to_string(), bytes));
    }
    // Resolve and verify all downloads before changing the installed collection.
    for (name, bytes) in &downloads { fs::write(mods_dir.join(name), bytes).map_err(|e| e.to_string())?; }
    let downloaded: Vec<String> = downloads.into_iter().map(|(name, _)| name).collect();
    // Only remove files previously managed by Fest, never arbitrary user mods.
    for old in previous { if !downloaded.contains(&old) && Path::new(&old).file_name().and_then(|x| x.to_str()) == Some(&old) { let _ = fs::remove_file(mods_dir.join(old)); } }
    fs::write(manifest_path, serde_json::to_vec_pretty(&downloaded).unwrap()).map_err(|e| e.to_string())?;
    Ok(())
}
#[tokio::main]
async fn main() {
    let root = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("FestLauncher");
    let app = App { root, status: Arc::new(Mutex::new(Status { phase: "idle".into(), message: "Всё готово к запуску".into(), busy: false })) };
    // Embed the complete interface so the release executable is portable and
    // does not depend on the current working directory or a separate web folder.
    let router = Router::new()
        .route("/api/settings", get(get_settings).post(save))
        .route("/api/status", get(get_status))
        .route("/api/versions", get(versions))
        .route("/api/launch", post(launch))
        .route("/", get(|| async { ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], &include_bytes!("../web/index.html")[..]) }))
        .route("/style.css", get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], &include_bytes!("../web/style.css")[..]) }))
        .route("/app.js", get(|| async { ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], &include_bytes!("../web/app.js")[..]) }))
        .route("/assets/hero-orange.png", get(|| async { ([(header::CONTENT_TYPE, "image/png")], &include_bytes!("../web/assets/hero-orange.png")[..]) }))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3210").await.expect("Порт 3210 занят");
    println!("FestVisuals: http://127.0.0.1:3210");
    #[cfg(target_os = "windows")]
    {
        // Opening a browser is best effort; the URL above remains usable if it fails.
        let _ = Command::new("cmd").args(["/C", "start", "", "http://127.0.0.1:3210"]).spawn();
    }
    axum::serve(listener, router).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_names_and_memory() {
        let mut s = Settings::default();
        assert!(validate(&s).is_ok());
        s.username = "bad name".into();
        assert!(validate(&s).is_err());
        s.username = "Player".into();
        s.memory = 33;
        assert!(validate(&s).is_err());
    }
    #[test]
    fn iris_needs_sodium_and_paths_are_not_versions() {
        let mut s = Settings::default();
        s.mods.retain(|m| m != "sodium");
        assert!(validate(&s).is_err());
        s.mods.clear();
        s.version = "../other".into();
        assert!(validate(&s).is_err());
    }
}
