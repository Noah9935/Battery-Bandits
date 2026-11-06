// Ohne diese Zeile bemerkt sqlx::migrate! neue Migrationsdateien nicht.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
