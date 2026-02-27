use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// ── state.json ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PetState {
    pub state: String,
    pub detail: Option<String>,
    pub progress: Option<f64>,
    pub updated_at: Option<String>,
}

// ── layers.json input ──

#[derive(Debug, Deserialize)]
struct CfgFile {
    width: Option<u32>,
    height: Option<u32>,
    character: Option<CharCfg>,
    layers: Option<Vec<LayerCfg>>,
    sprites: Option<SpritesCfg>,
}

#[derive(Debug, Deserialize)]
struct CharCfg {
    x: Option<f64>,
    y: Option<f64>,
    scale: Option<f64>,
    depth: Option<i32>,
    wander: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct LayerCfg {
    image: String,
    x: Option<f64>,
    y: Option<f64>,
    depth: Option<i32>,
    scale: Option<f64>,
    alpha: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SpritesCfg {
    frame_width: Option<u32>,
    frame_height: Option<u32>,
    anims: Option<HashMap<String, AnimCfg>>,
}

#[derive(Debug, Deserialize)]
struct AnimCfg {
    file: String,
    frames: Option<u32>,
    rate: Option<u32>,
    #[serde(default = "neg_one")]
    repeat: i32,
}

fn neg_one() -> i32 {
    -1
}

// ── IPC response ──

#[derive(Debug, Serialize)]
struct FullData {
    width: u32,
    height: u32,
    character: CharData,
    layers: Vec<LayerItem>,
    sprites: Option<SpritesData>,
}

#[derive(Debug, Serialize)]
struct CharData {
    x: f64,
    y: f64,
    scale: f64,
    depth: i32,
    wander: f64,
}

#[derive(Debug, Serialize)]
struct LayerItem {
    data_url: String,
    x: f64,
    y: f64,
    depth: i32,
    scale: f64,
    alpha: f64,
}

#[derive(Debug, Serialize)]
struct SpritesData {
    frame_width: u32,
    frame_height: u32,
    anims: Vec<AnimItem>,
}

#[derive(Debug, Serialize)]
struct AnimItem {
    key: String,
    data_url: String,
    frames: u32,
    rate: u32,
    repeat: i32,
}

// ── app state ──

struct AppPaths {
    state_path: PathBuf,
    layers_dir: PathBuf,
}

// ── commands ──

#[tauri::command]
fn read_state(paths: tauri::State<'_, Mutex<AppPaths>>) -> Result<PetState, String> {
    let p = paths.lock().map_err(|e| e.to_string())?;
    let raw = fs::read_to_string(&p.state_path)
        .map_err(|e| format!("{}: {e}", p.state_path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse: {e}"))
}

fn encode_image(path: &PathBuf) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let mime = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", B64.encode(&bytes)))
}

#[tauri::command]
fn load_layers(paths: tauri::State<'_, Mutex<AppPaths>>) -> Result<FullData, String> {
    let p = paths.lock().map_err(|e| e.to_string())?;
    let cfg_path = p.layers_dir.join("layers.json");

    let cfg: CfgFile = if cfg_path.exists() {
        let raw = fs::read_to_string(&cfg_path).map_err(|e| format!("layers.json: {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("layers.json: {e}"))?
    } else {
        CfgFile {
            width: None,
            height: None,
            character: None,
            layers: None,
            sprites: None,
        }
    };

    let w = cfg.width.unwrap_or(200);
    let h = cfg.height.unwrap_or(250);

    let cc = cfg.character.unwrap_or(CharCfg {
        x: None,
        y: None,
        scale: None,
        depth: None,
        wander: None,
    });
    let character = CharData {
        x: cc.x.unwrap_or(w as f64 / 2.0),
        y: cc.y.unwrap_or(h as f64 * 0.66),
        scale: cc.scale.unwrap_or(2.5),
        depth: cc.depth.unwrap_or(0),
        wander: cc.wander.unwrap_or(18.0),
    };

    // layers
    let mut layer_items = Vec::new();
    for entry in cfg.layers.unwrap_or_default() {
        let img_path = p.layers_dir.join(&entry.image);
        if !img_path.exists() {
            eprintln!("⚠️  Layer not found: {}", img_path.display());
            continue;
        }
        layer_items.push(LayerItem {
            data_url: encode_image(&img_path)?,
            x: entry.x.unwrap_or(w as f64 / 2.0),
            y: entry.y.unwrap_or(h as f64 / 2.0),
            depth: entry.depth.unwrap_or(-1),
            scale: entry.scale.unwrap_or(1.0),
            alpha: entry.alpha.unwrap_or(1.0),
        });
    }

    // sprites
    let sprites_data = if let Some(scfg) = cfg.sprites {
        let fw = scfg.frame_width.unwrap_or(32);
        let fh = scfg.frame_height.unwrap_or(32);
        let mut anims = Vec::new();

        for (key, acfg) in scfg.anims.unwrap_or_default() {
            let img_path = p.layers_dir.join(&acfg.file);
            if !img_path.exists() {
                eprintln!("⚠️  Sprite not found: {}", img_path.display());
                continue;
            }
            anims.push(AnimItem {
                key,
                data_url: encode_image(&img_path)?,
                frames: acfg.frames.unwrap_or(1),
                rate: acfg.rate.unwrap_or(4),
                repeat: acfg.repeat,
            });
        }

        Some(SpritesData {
            frame_width: fw,
            frame_height: fh,
            anims,
        })
    } else {
        None
    };

    Ok(FullData {
        width: w,
        height: h,
        character,
        layers: layer_items,
        sprites: sprites_data,
    })
}

// ── bootstrap ──

fn find_project_root() -> PathBuf {
    if let Ok(p) = std::env::var("STAR_PROJECT_ROOT") {
        return PathBuf::from(p);
    }
    let mut dir = std::env::current_dir().unwrap_or_default();
    for _ in 0..5 {
        if dir.join("state.json").exists() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let root = find_project_root();
    eprintln!("📦 State : {}", root.join("state.json").display());
    eprintln!("🎨 Layers: {}", root.join("layers").display());

    tauri::Builder::default()
        .manage(Mutex::new(AppPaths {
            state_path: root.join("state.json"),
            layers_dir: root.join("layers"),
        }))
        .invoke_handler(tauri::generate_handler![read_state, load_layers])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
