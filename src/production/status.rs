use super::Config;

#[cfg(windows)]
mod win {
 use super::*;
 use std::{ffi::{c_void,OsStr},os::windows::ffi::OsStrExt,ptr};
 #[link(name="winspool")]
 unsafe extern "system"{
  fn OpenPrinterW(name:*const u16,handle:*mut *mut c_void,defaults:*mut c_void)->i32;
  fn GetPrinterW(handle:*mut c_void,level:u32,buffer:*mut u8,size:u32,needed:*mut u32)->i32;
  fn ClosePrinter(handle:*mut c_void)->i32;
  fn GetDefaultPrinterW(buffer:*mut u16,size:*mut u32)->i32;
 }
 fn wide(s:&str)->Vec<u16>{OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()}
 fn default_printer()->Option<String>{unsafe {
  let mut size=0u32;GetDefaultPrinterW(ptr::null_mut(),&mut size);
  if size==0||size>32768{return None}
  let mut buffer=vec![0u16;size as usize];
  if GetDefaultPrinterW(buffer.as_mut_ptr(),&mut size)==0{return None}
  Some(String::from_utf16_lossy(&buffer[..buffer.iter().position(|&v|v==0).unwrap_or(buffer.len())]))
 }}
 pub fn printer_status(c:&Config)->(String,bool,&'static str,u32){
  let name=if c.printer_name.trim().is_empty(){default_printer().unwrap_or_default()}else{c.printer_name.clone()};
  if name.is_empty(){return ("(no Windows printer)".into(),false,"unknown",0)}
  let mut handle: *mut c_void=ptr::null_mut();
  let valid=unsafe{OpenPrinterW(wide(&name).as_ptr(),&mut handle,ptr::null_mut())!=0};
  if !valid{return (name,false,"unknown",0)}
  let mut flag=0u32;let mut needed=0u32;
  unsafe {
   let _=GetPrinterW(handle,6,&mut flag as *mut u32 as *mut u8,4,&mut needed);
   ClosePrinter(handle);
  }
  let paper=if flag&0x10!=0{"paper_out"}else if flag&(0x8|0x40)!=0{"paper_problem"}else{"unknown"};
  (name,true,paper,flag)
 }
}
#[cfg(windows)]pub use win::printer_status;
#[cfg(not(windows))]pub fn printer_status(c:&Config)->(String,bool,&'static str,u32){(c.printer_name.clone(),false,"unknown",0)}
