fn main() {
    println!("cargo:rerun-if-changed=templates/*");

    let dir: String = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let input = format!("{dir}/style.css");
    let output = format!("{dir}/static/style.css");

    match std::process::Command::new("tailwindcss")
        .args(["-m", "--input", &input, "--output", &output])
        .output()
        {
            Ok(result) => {
                if !result.status.success() {
                    let error = String::from_utf8_lossy(&result.stderr);
                    println!("cargo:warning=tailwindcss returned {}", result.status);
                    println!("cargo:warning=Unable to build CSS !");
                    println!("cargo:warning=Output: {error}");
                }
            },
            Err(e) => {
                println!("cargo:warning=Could not run tailwindcss: {}", e);
            },
        }
}
