fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target_os not specified");

    if target_os != "linux" && target_os != "android" {
        return;
    }

    let mut build = cc::Build::new();

    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("target_arch not specified");

    build.include(format!("libunwind/{}", target_arch));
    build.file(format!("libunwind/{}/getcontext.S", target_arch));

    build.compile("ucontext");
}
