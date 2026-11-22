use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=SPARKLE_FRAMEWORK_DIR");
    println!("cargo:rerun-if-changed=native/updater.m");
    if env::var_os("CARGO_FEATURE_SPARKLE").is_none() {
        return;
    }
    let framework = env::var("SPARKLE_FRAMEWORK_DIR")
        .expect("set SPARKLE_FRAMEWORK_DIR for Sparkle builds");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("planning_updater.o");
    let archive = out.join("libplanning_updater.a");
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        other => panic!("Sparkle builds require Apple Silicon, found {other}"),
    };
    let status = Command::new("clang")
        .args(["-fobjc-arc", "-arch", arch, "-mmacosx-version-min=11.0", "-F", &framework,
               "-c", "native/updater.m", "-o"])
        .arg(&object)
        .status()
        .unwrap();
    assert!(status.success(), "could not compile native/updater.m");
    assert!(Command::new("ar").arg("rcs").arg(&archive).arg(&object).status().unwrap().success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=planning_updater");
    println!("cargo:rustc-link-search=framework={framework}");
    println!("cargo:rustc-link-lib=framework=Sparkle");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
}
