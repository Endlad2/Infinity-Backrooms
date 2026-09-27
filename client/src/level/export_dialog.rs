//! Диалог «Сохранить как» через нативный API Windows.
//!
//! Используем `GetSaveFileNameW` (comdlg32) — стандартный save-dialog.
//! Если пользователь отменил — возвращаем None.

use std::path::PathBuf;

/// Показать нативный save-dialog. Возвращает путь, куда сохранять.
/// Если пользователь отменил или диалог не поддерживается — None.
pub fn show_save_dialog(default_name: &str) -> Option<PathBuf> {
    imp::show_save_dialog(default_name)
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetSaveFileNameW(lpofn: *mut OpenFileNameW) -> i32;
        fn CommDlgExtendedError() -> u32;
    }

    #[repr(C)]
    struct OpenFileNameW {
        l_struct_size: u32,
        hwnd_owner: *mut core::ffi::c_void,
        h_instance: *mut core::ffi::c_void,
        lpstr_filter: *const u16,
        lpstr_custom_filter: *const u16,
        n_max_cust_filter: u32,
        n_filter_index: u32,
        lpstr_file: *mut u16,
        n_max_file: u32,
        lpstr_file_title: *const u16,
        n_max_file_title: u32,
        lpstr_def_ext: *const u16,
        lpstr_initial_dir: *const u16,
        lpstr_title: *const u16,
        flags: u32,
        n_file_offset: u16,
        n_file_extension: u16,
        lpstr_def_ext2: *const u16,
        lpstr_custom_data: *const u16,
        lpfn_hook: *const core::ffi::c_void,
        lpstr_template_name: *const u16,
        pv_reserved: *mut core::ffi::c_void,
        dw_reserved: u32,
        flags_ex: u32,
    }

    // OFN_* флаги.
    const OFN_OVERWRITEPROMPT: u32 = 0x00000002;
    const OFN_PATHMUSTEXIST: u32 = 0x00000800;
    const OFN_EXPLORER: u32 = 0x00080000;
    const OFN_NOCHANGEDIR: u32 = 0x00000008;

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn show_save_dialog(default_name: &str) -> Option<PathBuf> {
        // Фильтр: "Zip archives (*.zip)\0*.zip\0All files (*.*)\0*.*\0\0"
        let filter_wide: Vec<u16> = "Zip archives (*.zip)\0*.zip\0All files (*.*)\0*.*\0\0"
            .encode_utf16()
            .collect();

        let mut buf: Vec<u16> = to_wide_null(default_name);
        buf.resize(2048, 0);

        let title = to_wide_null("Save level as...");
        let def_ext = to_wide_null("zip");

        let mut ofn = OpenFileNameW {
            l_struct_size: std::mem::size_of::<OpenFileNameW>() as u32,
            hwnd_owner: std::ptr::null_mut(),
            h_instance: std::ptr::null_mut(),
            lpstr_filter: filter_wide.as_ptr(),
            lpstr_custom_filter: std::ptr::null(),
            n_max_cust_filter: 0,
            n_filter_index: 1,
            lpstr_file: buf.as_mut_ptr(),
            n_max_file: buf.len() as u32,
            lpstr_file_title: std::ptr::null(),
            n_max_file_title: 0,
            lpstr_def_ext: def_ext.as_ptr(),
            lpstr_initial_dir: std::ptr::null(),
            lpstr_title: title.as_ptr(),
            flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_EXPLORER | OFN_NOCHANGEDIR,
            n_file_offset: 0,
            n_file_extension: 0,
            lpstr_def_ext2: std::ptr::null(),
            lpstr_custom_data: std::ptr::null(),
            lpfn_hook: std::ptr::null(),
            lpstr_template_name: std::ptr::null(),
            pv_reserved: std::ptr::null_mut(),
            dw_reserved: 0,
            flags_ex: 0,
        };

        let ok = unsafe { GetSaveFileNameW(&mut ofn) };
        if ok == 0 {
            let _ = unsafe { CommDlgExtendedError() };
            return None;
        }

        // buf теперь содержит путь, terminated by 0.
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        let s = String::from_utf16_lossy(&buf[..len]);
        if s.is_empty() { None } else { Some(PathBuf::from(s)) }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;
    pub fn show_save_dialog(_default_name: &str) -> Option<PathBuf> {
        // На не-Windows платформах сохраняем в %TEMP%.
        None
    }
}
