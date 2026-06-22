use codestudio_lite_lib::core::asar_archive;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    let asar_path = args
        .get(1)
        .expect("usage: verify_claude_asar <path-to-app.asar>");
    let out_path = args.get(2).expect("usage: verify_claude_asar <in> <out>");

    let bytes = fs::read(asar_path).expect("read asar");
    println!("orig_asar_size={}", bytes.len());

    let (pkg_text, orig_main) = asar_archive::read_package_json(&bytes).expect("read pkg");
    println!("original_main={}", orig_main);
    assert_eq!(orig_main, ".vite/build/index.pre.js");

    let mut pkg: serde_json::Value = serde_json::from_str(&pkg_text).expect("parse pkg");
    let obj = pkg.as_object_mut().expect("pkg object");
    obj.insert(
        "main".to_string(),
        serde_json::Value::from("_csl_inspector_shim.js"),
    );
    obj.insert(
        "originalMain".to_string(),
        serde_json::Value::from(orig_main.as_str()),
    );
    let new_pkg = serde_json::to_string_pretty(&pkg).expect("serialize pkg");

    let shim = format!(
        "try {{ require('node:inspector').open(9233); }} catch (e) {{ process.stderr.write('[csl] inspector open failed: ' + (e && e.message) + '\\n'); }}\ntry {{ require('./{}'); }} catch (e) {{ process.stderr.write('[csl] main load failed: ' + (e && e.message) + '\\n'); }}\n",
        orig_main
    );

    let patched = asar_archive::build_patched_asar(
        &bytes,
        new_pkg.as_bytes(),
        "_csl_inspector_shim.js",
        shim.as_bytes(),
    )
    .expect("build patched");
    println!("patched_asar_size={}", patched.len());

    fs::write(out_path, &patched).expect("write patched");
    println!("wrote={}", out_path);

    // Verify the patched asar reads back correctly.
    let (text2, main2) = asar_archive::read_package_json(&patched).expect("read back pkg");
    assert_eq!(main2, "_csl_inspector_shim.js");
    assert!(text2.contains("\"originalMain\""));
    assert!(text2.contains(".vite/build/index.pre.js"));
    println!("readback_ok main={}", main2);

    // Verify a known original file (index.pre.js) is still at its offset and
    // readable. Its entry lives at .vite.files.build.files.index.pre.js in the
    // tree; read via the tree.
    let header = asar_archive::read_header(&patched).expect("read header");
    let build = header
        .tree
        .get("files")
        .and_then(|f| f.get(".vite"))
        .and_then(|v| v.get("files"))
        .and_then(|f| f.get("build"))
        .and_then(|b| b.get("files"))
        .expect("build files");
    let pre = build.get("index.pre.js").expect("index.pre.js entry");
    let off = pre
        .get("offset")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<usize>().ok())
        .expect("offset");
    let size = pre.get("size").and_then(serde_json::Value::as_u64).unwrap() as usize;
    let base = header.content_base as usize;
    let pre_bytes = &patched[base + off..base + off + size];
    // index.pre.js is JS source; first bytes should be printable.
    let head = String::from_utf8_lossy(&pre_bytes[..16.min(pre_bytes.len())]);
    println!(
        "index.pre.js readable at offset {} size {} head={:?}",
        off, size, head
    );
    println!("SUCCESS");
}
