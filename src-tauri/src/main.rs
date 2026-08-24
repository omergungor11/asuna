// Windows release build'inde ek konsol penceresini engeller — SILME.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    asuna_lib::run()
}
