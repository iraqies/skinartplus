mod commands;
mod mc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::claim::ClaimState::default())
        .invoke_handler(tauri::generate_handler![
            commands::files::select_image,
            commands::files::select_base_skin,
            commands::files::select_original_skin,
            commands::files::select_export_dir,
            commands::files::open_url,
            commands::files::open_namemc_cache,
            commands::files::read_file_base64,
            commands::files::save_temp_buffer,
            commands::image::generate_all,
            commands::auth::start_auth_device,
            commands::auth::poll_auth_token,
            commands::auth::refresh_saved_token,
            commands::auth::fetch_profile,
            commands::auth::download_skin_texture,
            commands::auth::fetch_avatar,
            commands::auth::download_head,
            commands::namemc::get_uuid_from_name,
            commands::namemc::scrape_namemc_skin,
            commands::namemc::scrape_namemc_all_skins,
            commands::namemc::upload_one_skin,
            commands::templates::load_templates,
            commands::templates::get_template_image_path,
            commands::templates::get_template_image_data,
            commands::accounts::load_accounts,
            commands::accounts::save_account,
            commands::accounts::delete_account,
            commands::claim::claim_namemc,
            commands::claim::cancel_claim,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = commands::templates::sync_bundled(&handle);
                commands::templates::sync_remote(handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
