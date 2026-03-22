use std::{env, path::PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let root = manifest_dir
        .join("../../third_party/doomgeneric/doomgeneric")
        .canonicalize()
        .expect("locate doomgeneric sources");

    println!("cargo:rerun-if-changed={}", root.display());

    let sources = [
        "dummy.c",
        "am_map.c",
        "doomdef.c",
        "doomstat.c",
        "dstrings.c",
        "d_event.c",
        "d_items.c",
        "d_iwad.c",
        "d_loop.c",
        "d_main.c",
        "d_mode.c",
        "d_net.c",
        "f_finale.c",
        "f_wipe.c",
        "g_game.c",
        "hu_lib.c",
        "hu_stuff.c",
        "info.c",
        "i_cdmus.c",
        "i_endoom.c",
        "i_joystick.c",
        "i_scale.c",
        "i_sound.c",
        "i_system.c",
        "i_timer.c",
        "memio.c",
        "m_argv.c",
        "m_bbox.c",
        "m_cheat.c",
        "m_config.c",
        "m_controls.c",
        "m_fixed.c",
        "m_menu.c",
        "m_misc.c",
        "m_random.c",
        "p_ceilng.c",
        "p_doors.c",
        "p_enemy.c",
        "p_floor.c",
        "p_inter.c",
        "p_lights.c",
        "p_map.c",
        "p_maputl.c",
        "p_mobj.c",
        "p_plats.c",
        "p_pspr.c",
        "p_saveg.c",
        "p_setup.c",
        "p_sight.c",
        "p_spec.c",
        "p_switch.c",
        "p_telept.c",
        "p_tick.c",
        "p_user.c",
        "r_bsp.c",
        "r_data.c",
        "r_draw.c",
        "r_main.c",
        "r_plane.c",
        "r_segs.c",
        "r_sky.c",
        "r_things.c",
        "sha1.c",
        "sounds.c",
        "statdump.c",
        "st_lib.c",
        "st_stuff.c",
        "s_sound.c",
        "tables.c",
        "v_video.c",
        "wi_stuff.c",
        "w_checksum.c",
        "w_file.c",
        "w_file_stdc.c",
        "w_main.c",
        "w_wad.c",
        "z_zone.c",
        "i_input.c",
        "i_video.c",
        "doomgeneric.c",
        "doomgeneric_framefeed.c",
        "mus2mid.c",
    ];

    let mut build = cc::Build::new();
    build
        .include(&root)
        .warnings(false)
        .flag_if_supported("-std=c99")
        .define("HAVE_CONFIG_H", None)
        .define("FEATURE_SOUND", None)
        .define("NORMALUNIX", None)
        .define("LINUX", None)
        .define("SNDSERV", None)
        .define("_DEFAULT_SOURCE", None)
        .define("_POSIX_C_SOURCE", Some("200809L"));
    if cfg!(target_os = "macos") {
        build.flag("-mmacosx-version-min=11.0");
    }

    for file in &sources {
        let path = root.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(path);
    }

    build.compile("doomgeneric_core");
}
