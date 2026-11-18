//! Builds the bridge to libcdr (CorelDRAW files) when libcdr and librevenge are installed.
//! Their prefixes come from `LIBCDR_PREFIX` and `LIBREVENGE_PREFIX`, else from Homebrew.
//! Without them the crate still builds, and reading a CorelDRAW file returns an error
//! (`LIBCDR_PREFIX=/none` builds that way on purpose).

use std::path::PathBuf;
use std::process::Command;

fn prefix(env: &str, formula: &str) -> Option<PathBuf> {
    println!("cargo::rerun-if-env-changed={env}");
    // Set, it is the only place looked at; a folder without headers means "not installed".
    if let Some(p) = std::env::var_os(env) {
        return Some(PathBuf::from(p)).filter(|p| p.join("include").is_dir());
    }
    let brew = Command::new("brew").args(["--prefix", formula]).output().ok().filter(|o| o.status.success());
    let from_brew = brew.map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
    from_brew.into_iter().chain(["/opt/homebrew/opt", "/usr/local/opt"].map(|d| PathBuf::from(d).join(formula))).find(|p| p.join("include").is_dir())
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(cdr)");
    println!("cargo::rerun-if-changed=cdr/shim.cpp");
    let (Some(cdr), Some(revenge)) = (prefix("LIBCDR_PREFIX", "libcdr"), prefix("LIBREVENGE_PREFIX", "librevenge")) else {
        println!("cargo::warning=libcdr not found (brew install libcdr): CorelDRAW files cannot be opened");
        return;
    };
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("cdr/shim.cpp")
        .include(cdr.join("include/libcdr-0.1"))
        .include(revenge.join("include/librevenge-0.0"))
        .warnings(false)
        .compile("vs_cdr");
    for dir in [&cdr, &revenge] {
        println!("cargo::rustc-link-search=native={}", dir.join("lib").display());
    }
    for lib in ["cdr-0.1", "revenge-0.0", "revenge-stream-0.0"] {
        println!("cargo::rustc-link-lib=dylib={lib}");
    }
    println!("cargo::rustc-cfg=cdr");
}
