extern crate bindgen;

use bindgen::builder;

fn gen() {
    let name = "cras_common";
    let bindings = builder()
        .header("c_headers/cras_messages.h")
        .header("c_headers/cras_types.h")
        .header("c_headers/cras_audio_format.h")
        .whitelist_type("cras_.*")
        .whitelist_var("cras_.*")
        .whitelist_type("CRAS_.*")
        .whitelist_var("CRAS_.*")
        .whitelist_type("audio_message")
        .constified_enum_module("CRAS_.*")
        .constified_enum_module("_snd_pcm_.*")
        .generate()
        .expect(format!("Unable to generate {} code", name).as_str());
    bindings
        .write_to_file("lib_gen.rs")
        .expect("Unable to generate lib.rs file");
}

fn main() {
    gen();
}
