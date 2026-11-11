//! Windows DPAPI per-user vault. No secret is passed via command line or printed.
#[derive(serde::Serialize,serde::Deserialize)]
pub struct Credentials { pub db_password:String,pub mics_sync_token:String }
#[cfg(windows)]
mod win {
    use super::Credentials;
    use std::{ffi::c_void,fs,path::PathBuf,ptr};
    #[repr(C)]
    struct DataBlob { cb_data:u32,pb_data:*mut u8 }
    #[link(name="crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(data:*const DataBlob,desc:*const u16,entropy:*const DataBlob,reserved:*mut c_void,
            prompt:*const c_void,flags:u32,out:*mut DataBlob)->i32;
        fn CryptUnprotectData(data:*const DataBlob,desc:*mut *mut u16,entropy:*const DataBlob,reserved:*mut c_void,
            prompt:*const c_void,flags:u32,out:*mut DataBlob)->i32;
    }
    #[link(name="kernel32")]
    unsafe extern "system" { fn LocalFree(mem:*mut c_void)->*mut c_void; }
    const UI_FORBIDDEN:u32=0x1;
    pub fn path()->Result<PathBuf,String>{
        let root=std::env::var_os("LOCALAPPDATA").ok_or("Không tìm thấy LOCALAPPDATA")?;
        Ok(PathBuf::from(root).join("MICS-Print").join("credentials.dpapi"))
    }
    fn transform(bytes:&[u8],protect:bool)->Result<Vec<u8>,String>{
        if bytes.is_empty()||bytes.len()>1024*1024{return Err("Dữ liệu credentials không hợp lệ".into())}
        let input=DataBlob{cb_data:bytes.len() as u32,pb_data:bytes.as_ptr() as *mut u8};
        let mut output=DataBlob{cb_data:0,pb_data:ptr::null_mut()};
        let ok=unsafe{
            if protect {
                CryptProtectData(&input,ptr::null(),ptr::null(),ptr::null_mut(),ptr::null(),UI_FORBIDDEN,&mut output)
            }else{
                CryptUnprotectData(&input,ptr::null_mut(),ptr::null(),ptr::null_mut(),ptr::null(),UI_FORBIDDEN,&mut output)
            }
        };
        if ok==0 {return Err("DPAPI không thể mã hóa/giải mã cho tài khoản Windows hiện tại".into())}
        let result=unsafe{std::slice::from_raw_parts(output.pb_data,output.cb_data as usize).to_vec()};
        unsafe{LocalFree(output.pb_data as *mut c_void);}
        Ok(result)
    }
    pub fn save(c:&Credentials)->Result<(),String>{
        if c.db_password.trim().is_empty()||c.mics_sync_token.trim().is_empty(){return Err("Cần nhập đầy đủ mật khẩu Firebird và MICS_SYNC_TOKEN".into())}
        let bytes=serde_json::to_vec(c).map_err(|e|e.to_string())?;
        let encrypted=transform(&bytes,true)?;
        let file=path()?;let dir=file.parent().ok_or("Đường dẫn không hợp lệ")?;
        fs::create_dir_all(dir).map_err(|e|e.to_string())?;
        let tmp=file.with_extension("dpapi.tmp");
        fs::write(&tmp,encrypted).map_err(|e|format!("Không ghi file mã hóa: {e}"))?;
        // Keep old vault intact on failure; replace under same Windows user.
        if file.exists(){
            let backup=file.with_extension("dpapi.bak");
            fs::copy(&file,&backup).map_err(|e|e.to_string())?;
            if let Err(e)=fs::remove_file(&file){let _=fs::remove_file(&tmp);return Err(e.to_string())}
            if let Err(e)=fs::rename(&tmp,&file){let _=fs::copy(&backup,&file);return Err(e.to_string())}
        }else{fs::rename(&tmp,&file).map_err(|e|e.to_string())?}
        Ok(())
    }
    pub fn load()->Result<Credentials,String>{
        let file=path()?;
        let encrypted=fs::read(&file).map_err(|_|"Chưa thiết lập MICS-Credentials.exe cho tài khoản Windows đang chạy Agent".to_owned())?;
        let plain=transform(&encrypted,false)?;
        serde_json::from_slice(&plain).map_err(|_|"Dữ liệu vault không hợp lệ".into())
    }
    pub fn configured()->bool{path().ok().map(|p|p.is_file()).unwrap_or(false)}
}
#[cfg(windows)]
pub use win::{configured,load,save};
