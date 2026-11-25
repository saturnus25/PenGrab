use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/PenGrab.ico");
    println!("cargo:rerun-if-changed=assets/PenGrab.rc");
    for variable in ["PATH", "WindowsSdkVerBinPath", "ProgramFiles(x86)"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        return;
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("PenGrab.res");
    let compile = |program: PathBuf| {
        Command::new(program)
            .args(["/nologo", "/fo"])
            .arg(&output)
            .arg("PenGrab.rc")
            .current_dir("assets")
            .status()
    };
    let status = match compile(PathBuf::from("rc.exe")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(path) = sdk_resource_compiler() {
                compile(path)
            } else {
                println!(
                    "cargo:warning=Windows SDK resource compiler unavailable. The executable will omit its icon."
                );
                return;
            }
        }
        status => status,
    };
    assert!(
        status.is_ok_and(|status| status.success()),
        "could not compile the PenGrab icon"
    );
    println!("cargo:rustc-link-arg-bin=pengrab={}", output.display());
}

fn sdk_resource_compiler() -> Option<PathBuf> {
    let host = env::var("HOST").ok()?;
    let architecture = match host.split('-').next()? {
        "aarch64" => "arm64",
        "i686" => "x86",
        _ => "x64",
    };
    let compiler = |bin: PathBuf| bin.join(architecture).join("rc.exe");
    if let Some(path) = env::var_os("WindowsSdkVerBinPath")
        .map(PathBuf::from)
        .map(compiler)
        .filter(|path| path.is_file())
    {
        return Some(path);
    }
    let sdk = PathBuf::from(env::var_os("ProgramFiles(x86)")?).join("Windows Kits/10/bin");
    fs::read_dir(sdk)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| compiler(entry.path()))
        .filter(|path| path.is_file())
        .max()
}
