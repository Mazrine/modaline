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
