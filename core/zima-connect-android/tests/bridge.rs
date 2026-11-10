use serde_json::Value;

#[test]
fn json_api_roundtrip() {
    let dir = std::env::temp_dir().join(format!("zc-android-{}", std::process::id()));
    let init = zima_connect_android::call(
        "init",
        &serde_json::json!({
            "data_dir": dir.join("data"),
            "download_dir": dir.join("dl"),
            "device_name": "Pixel"
        })
        .to_string(),
    );
    let v: Value = serde_json::from_str(&init).unwrap();
    assert!(v["ok"]["id"].as_str().unwrap().len() > 20, "{init}");

    let contacts: Value =
        serde_json::from_str(&zima_connect_android::call("contacts", "")).unwrap();
    assert_eq!(contacts["ok"], serde_json::json!([]));

    let household: Value =
        serde_json::from_str(&zima_connect_android::call("household", "")).unwrap();
    assert!(household["ok"].is_null());

    let bad: Value = serde_json::from_str(&zima_connect_android::call(
        "join",
        "{\"code\":\"nonsense\"}",
    ))
    .unwrap();
    assert!(
        bad["err"].as_str().unwrap().contains("server code"),
        "{bad}"
    );

    let unknown: Value = serde_json::from_str(&zima_connect_android::call("nope", "")).unwrap();
    assert!(unknown["err"].is_string());
    assert!(zima_connect_android::next_event(10).is_none());
}
