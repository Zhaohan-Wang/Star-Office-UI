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

// ── map.json input ──

#[derive(Debug, Deserialize)]
struct MapCfgFile {
    tile_size: Option<u32>,
    cols: Option<u32>,
    rows: Option<u32>,
    zoom: Option<u32>,
    tileset: String,
    character_speed: Option<f64>,
    ground: Vec<Vec<i32>>,
    border: Option<Vec<Vec<i32>>>,
    rug: Option<Vec<Vec<i32>>>,
    objects: Vec<Vec<i32>>,
    collision: Vec<Vec<u8>>,
    pois: Option<HashMap<String, PoiCfg>>,
    state_icons: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct PoiCfg {
    col: u32,
    row: u32,
}

// ── IPC responses ──

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

#[derive(Debug, Serialize)]
struct MapData {
    tile_size: u32,
    cols: u32,
    rows: u32,
    zoom: u32,
    tileset_url: String,
    tileset_cols: u32,
    character_speed: f64,
    ground: Vec<Vec<i32>>,
    border: Vec<Vec<i32>>,
    rug: Vec<Vec<i32>>,
    objects: Vec<Vec<i32>>,
    collision: Vec<Vec<u8>>,
    pois: HashMap<String, PoiOut>,
    state_icons: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct PoiOut {
    col: u32,
    row: u32,
}

// ── shared ──

struct AppPaths {
    state_path: PathBuf,
    layers_dir: PathBuf,
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

// ── commands ──

#[tauri::command]
fn read_state(paths: tauri::State<'_, Mutex<AppPaths>>) -> Result<PetState, String> {
    let p = paths.lock().map_err(|e| e.to_string())?;
    let raw = fs::read_to_string(&p.state_path)
        .map_err(|e| format!("{}: {e}", p.state_path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse: {e}"))
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
        x: None, y: None, scale: None, depth: None, wander: None,
    });
    let character = CharData {
        x: cc.x.unwrap_or(w as f64 / 2.0),
        y: cc.y.unwrap_or(h as f64 * 0.66),
        scale: cc.scale.unwrap_or(2.5),
        depth: cc.depth.unwrap_or(0),
        wander: cc.wander.unwrap_or(18.0),
    };

    let mut items = Vec::new();
    for entry in cfg.layers.unwrap_or_default() {
        let img_path = p.layers_dir.join(&entry.image);
        if !img_path.exists() {
            continue;
        }
        items.push(LayerItem {
            data_url: encode_image(&img_path)?,
            x: entry.x.unwrap_or(w as f64 / 2.0),
            y: entry.y.unwrap_or(h as f64 / 2.0),
            depth: entry.depth.unwrap_or(-1),
            scale: entry.scale.unwrap_or(1.0),
            alpha: entry.alpha.unwrap_or(1.0),
        });
    }

    let sprites_data = if let Some(scfg) = cfg.sprites {
        let fw = scfg.frame_width.unwrap_or(32);
        let fh = scfg.frame_height.unwrap_or(32);
        let mut anims = Vec::new();
        for (key, acfg) in scfg.anims.unwrap_or_default() {
            let img_path = p.layers_dir.join(&acfg.file);
            if !img_path.exists() {
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
        layers: items,
        sprites: sprites_data,
    })
}

#[tauri::command]
fn load_map(paths: tauri::State<'_, Mutex<AppPaths>>) -> Result<MapData, String> {
    let p = paths.lock().map_err(|e| e.to_string())?;
    let map_path = p.layers_dir.join("map.json");

    if !map_path.exists() {
        return Err("map.json not found".into());
    }

    let raw = fs::read_to_string(&map_path).map_err(|e| format!("map.json: {e}"))?;
    let cfg: MapCfgFile = serde_json::from_str(&raw).map_err(|e| format!("map.json: {e}"))?;

    let ts = cfg.tile_size.unwrap_or(16);
    let cols = cfg.cols.unwrap_or(cfg.ground.first().map_or(12, |r| r.len() as u32));
    let rows = cfg.rows.unwrap_or(cfg.ground.len() as u32);

    let tileset_path = p.layers_dir.join(&cfg.tileset);
    if !tileset_path.exists() {
        return Err(format!("tileset not found: {}", cfg.tileset));
    }
    let tileset_url = encode_image(&tileset_path)?;

    // figure out tileset column count from image width
    let img_bytes = fs::read(&tileset_path).map_err(|e| e.to_string())?;
    let tileset_cols = png_width(&img_bytes).unwrap_or(160) / ts;

    let mut pois = HashMap::new();
    for (k, v) in cfg.pois.unwrap_or_default() {
        pois.insert(k, PoiOut { col: v.col, row: v.row });
    }

    let icons_dir = p.layers_dir.join("Small (24x24) PNG");
    let mut state_icons = HashMap::new();
    for (state, filename) in cfg.state_icons.unwrap_or_default() {
        let path = icons_dir.join(&filename);
        if path.exists() {
            if let Ok(url) = encode_image(&path) {
                state_icons.insert(state, url);
            }
        }
    }

    Ok(MapData {
        tile_size: ts,
        cols,
        rows,
        zoom: cfg.zoom.unwrap_or(2),
        tileset_url,
        tileset_cols,
        character_speed: cfg.character_speed.unwrap_or(2.5),
        ground: cfg.ground,
        border: cfg.border.unwrap_or_default(),
        rug: cfg.rug.unwrap_or_default(),
        objects: cfg.objects,
        collision: cfg.collision,
        pois,
        state_icons,
    })
}

fn png_width(data: &[u8]) -> Option<u32> {
    if data.len() < 24 || &data[0..4] != b"\x89PNG" {
        return None;
    }
    Some(u32::from_be_bytes([data[16], data[17], data[18], data[19]]))
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
        .invoke_handler(tauri::generate_handler![read_state, load_layers, load_map])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
