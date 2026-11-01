// Neue Migrationen sollen immer einen Neubau auslösen: sqlx::migrate! bemerkt hinzugekommene Dateien sonst nicht.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
