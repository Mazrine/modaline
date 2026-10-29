use std::path::{Path, PathBuf};
use std::process::Command;

use vixel::color::Rgba;
use vixel::io;
use vixel::sprite::{ColorMode, Sprite, Tag};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vixel-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(dir: &Path) -> PathBuf {
    let mut s = Sprite::new(4, 4);
    s.add_frame_after(0);
    s.set(0, 0, 2, 2, Rgba::rgb(255, 0, 0));
    s.set(0, 1, 1, 1, Rgba::rgb(0, 0, 255));
    s.tags.push(Tag {
        name: "walk".into(),
        from: 0,
        to: 1,
        color: Rgba::rgb(1, 2, 3),
        direction: vixel::sprite::PlayDirection::Forward,
        repeat: 0,
        user_data: None,
    });
    let path = dir.join("in.vpx");
    io::save_project(&s, &path).unwrap();
    path
}

fn run(args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vixel"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn batch_runs_a_lua_script_then_saves() {
    let d = dir("script");
    let input = fixture(&d);
    let script = d.join("paint.lua");
    std::fs::write(
        &script,
        r#"
        app.activeSprite:setPixel(0, 0, {r=9, g=8, b=7, a=255})
        print("scripted", app.activeSprite.width)
        "#,
    )
    .unwrap();
    let out_path = d.join("out.vpx");
    let out = run(&[
        "-b".into(),
        input.display().to_string(),
        "--script".into(),
        script.display().to_string(),
        "--save-as".into(),
        out_path.display().to_string(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("scripted\t4"),
        "stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let saved = io::load_project(&out_path).unwrap();
    assert_eq!(saved.get(0, 0, 0, 0), Rgba::rgb(9, 8, 7));
    assert_eq!(saved.get(0, 0, 2, 2), Rgba::rgb(255, 0, 0));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn batch_exports_filtered_trimmed_sheet_and_atlas() {
    let d = dir("sheet");
    let input = fixture(&d);
    let sheet = d.join("sheet.png");
    let data = d.join("atlas.json");
    let out = run(&[
        "-b".into(),
        input.display().to_string(),
        "--frame-range".into(),
        "1".into(),
        "1".into(),
        "--trim".into(),
        "--scale".into(),
        "2".into(),
        "--sheet".into(),
        sheet.display().to_string(),
        "--data".into(),
        data.display().to_string(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sheet.exists());
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&data).unwrap()).unwrap();
    assert_eq!(meta["frames"].as_array().unwrap().len(), 1);
    assert_eq!(meta["frame_w"], 4, "2x2 content scaled 2x");
    assert_eq!(meta["frame_h"], 4);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn batch_lists_layers_tags_and_frames() {
    let d = dir("lists");
    let input = fixture(&d);
    let out = run(&[
        "-b".into(),
        input.display().to_string(),
        "--list-layers".into(),
        "--list-tags".into(),
        "--list-frames".into(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Layer 1"), "{text}");
    assert!(text.contains("walk"), "{text}");
    assert!(text.contains("100ms"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn batch_splits_layers_tags_and_palette() {
    let d = dir("split");
    let mut s = Sprite::new(2, 2);
    s.add_frame_after(0);
    s.add_layer_after(0);
    let top = s.layers.len() - 1;
    s.set(0, 0, 0, 0, Rgba::rgb(255, 0, 0));
    s.set(top, 1, 1, 1, Rgba::rgb(0, 0, 255));
    s.tags.push(Tag {
        name: "walk".into(),
        from: 0,
        to: 1,
        color: Rgba::rgb(1, 2, 3),
        direction: vixel::sprite::PlayDirection::Forward,
        repeat: 0,
        user_data: None,
    });
    let input = d.join("in.vpx");
    io::save_project(&s, &input).unwrap();
    let layers = d.join("layers");
    let tags = d.join("tags");
    let pal = d.join("pal.gpl");
    let out = run(&[
        "-b".into(),
        input.display().to_string(),
        "--split-layers".into(),
        layers.display().to_string(),
        "--split-tags".into(),
        tags.display().to_string(),
        "--palette".into(),
        pal.display().to_string(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read_dir(&layers).unwrap().count(), 4);
    assert_eq!(std::fs::read_dir(&tags).unwrap().count(), 2);
    let text = std::fs::read_to_string(&pal).unwrap();
    assert!(text.contains("GIMP Palette"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn batch_splits_grid_slices_and_tileset() {
    use vixel::sprite::{Slice, SliceKey, Tileset};
    let d = dir("split2");
    let mut s = Sprite::new(2, 2);
    s.slices.push(Slice {
        name: "head".into(),
        keys: vec![SliceKey {
            frame: 0,
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            center: None,
            pivot: None,
        }],
        ..Default::default()
    });
    s.tilesets.push(Tileset {
        id: 1,
        name: "tiles".into(),
        tile_w: 1,
        tile_h: 1,
        num_tiles: 1,
        image: vec![Rgba::rgb(9, 8, 7)],
        ..Default::default()
    });
    let input = d.join("in.vpx");
    io::save_project(&s, &input).unwrap();
    let grid = d.join("grid");
    let slices = d.join("slices");
    let tiles = d.join("tiles.png");
    let out = run(&[
        "-b".into(),
        input.display().to_string(),
        "--split-grid".into(),
        grid.display().to_string(),
        "--columns".into(),
        "2".into(),
        "--rows".into(),
        "1".into(),
        "--split-slices".into(),
        slices.display().to_string(),
        "--export-tileset".into(),
        tiles.display().to_string(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read_dir(&grid).unwrap().count(), 3, "2 png + meta");
    assert_eq!(std::fs::read_dir(&slices).unwrap().count(), 2, "png + meta");
    assert!(tiles.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn batch_converts_colour_mode_and_saves_as_project() {
    let d = dir("colour");
    let input = fixture(&d);
    let out_path = d.join("indexed.vpx");
    let out = run(&[
        "-b".into(),
        input.display().to_string(),
        "--color-mode".into(),
        "indexed".into(),
        "--dither".into(),
        "bayer4".into(),
        "--save-as".into(),
        out_path.display().to_string(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let back = io::load_project(&out_path).unwrap();
    assert_eq!(back.color_mode, ColorMode::Indexed);
    assert!(!back.palette.is_empty());
    let _ = std::fs::remove_dir_all(&d);
}
