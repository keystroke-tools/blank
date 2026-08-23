fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=frontend/dist");
    println!("cargo:rerun-if-changed=assets/missing-deployment.html");
}
