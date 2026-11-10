#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../production/mod.rs"] mod production;

#[cfg(windows)]
fn installed_printers() -> Vec<String> {
    // Gọi trực tiếp Windows Spooler, tránh khởi chạy PowerShell và tải .NET mỗi lần mở.
    use std::{ffi::c_void,ptr};
    #[repr(C)]
    struct PrinterInfo4 { name:*const u16, server:*const u16, attributes:u32 }
    #[link(name="winspool")]
    unsafe extern "system" {
        fn EnumPrintersW(flags:u32,name:*const u16,level:u32,buffer:*mut u8,size:u32,needed:*mut u32,returned:*mut u32)->i32;
    }
    fn wide(ptr:*const u16)->String {
        if ptr.is_null(){return String::new()}
        unsafe {let mut n=0;while n<32768 && *ptr.add(n)!=0{n+=1}
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr,n))}
    }
    unsafe {
        let mut size=0u32;let mut count=0u32;
        let _=EnumPrintersW(0x2|0x4,ptr::null(),4,ptr::null_mut(),0,&mut size,&mut count);
        if size==0||size>16*1024*1024{return Vec::new()}
        // Cấp phát vùng đệm theo alignment của cấu trúc, tránh UB trên Windows x64.
        let words=(size as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buffer=vec![0usize;words];
        if EnumPrintersW(0x2|0x4,ptr::null(),4,buffer.as_mut_ptr() as *mut u8,(words*std::mem::size_of::<usize>()) as u32,&mut size,&mut count)==0{return Vec::new()}
        let ptr=buffer.as_ptr() as *const PrinterInfo4;
        let mut names=Vec::new();
        for i in 0..(count as usize).min(4096){let v=wide((*ptr.add(i)).name);if !v.is_empty(){names.push(v)}}
        names.sort();names.dedup();names
    }
}

#[cfg(windows)]
fn pick_file(kind: &str) -> Option<String> {
    // Native Win32 common dialogs: không spawn PowerShell, không tải Forms/.NET.
    use std::{ffi::c_void,mem,ptr};
    #[repr(C)]
    struct OpenFileName {
        size:u32,owner:*mut c_void,instance:*mut c_void,filter:*const u16,custom_filter:*mut u16,
        max_custom_filter:u32,filter_index:u32,file:*mut u16,max_file:u32,file_title:*mut u16,
        max_file_title:u32,initial_dir:*const u16,title:*const u16,flags:u32,file_offset:u16,
        file_extension:u16,def_ext:*const u16,cust_data:isize,hook:*mut c_void,template_name:*const u16,
        reserved:*mut c_void,reserved_dword:u32,flags_ex:u32
    }
    #[repr(C)]
    struct BrowseInfo { owner:*mut c_void,root:*mut c_void,display_name:*mut u16,title:*const u16,
        flags:u32,callback:*mut c_void,lparam:isize,image:i32 }
    #[link(name="comdlg32")]
    unsafe extern "system" {fn GetOpenFileNameW(info:*mut OpenFileName)->i32;}
    #[link(name="shell32")]
    unsafe extern "system" {
        fn SHBrowseForFolderW(info:*const BrowseInfo)->*mut c_void;
        fn SHGetPathFromIDListW(id:*const c_void,path:*mut u16)->i32;
    }
    #[link(name="ole32")]
    unsafe extern "system" {fn CoTaskMemFree(ptr:*mut c_void);}
    fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(std::iter::once(0)).collect()}
    if kind=="folder" {
        let title=wide("Chọn thư mục lưu PDF / PDF保存先");
        let dialog=BrowseInfo{owner:ptr::null_mut(),root:ptr::null_mut(),display_name:ptr::null_mut(),
            title:title.as_ptr(),flags:0x00000040|0x00000001,callback:ptr::null_mut(),lparam:0,image:0};
        unsafe {
            let id=SHBrowseForFolderW(&dialog);
            if id.is_null(){return None}
            let mut path=vec![0u16;32768];
            let ok=SHGetPathFromIDListW(id,path.as_mut_ptr())!=0;
            CoTaskMemFree(id);
            if !ok{return None}
            let len=path.iter().position(|v|*v==0).unwrap_or(0);
            let value=String::from_utf16_lossy(&path[..len]);
            return if value.is_empty(){None}else{Some(value)}
        }
    }
    let filter=if kind=="xlsx"{
        // Hai chuỗi null liên tiếp theo chuẩn Windows.
        wide("Excel Workbook (*.xlsx)\0*.xlsx\0All Files (*.*)\0*.*\0")
    }else{wide("Executable (*.exe)\0*.exe\0All Files (*.*)\0*.*\0")};
    let mut file=vec![0u16;32768];
    let mut info=OpenFileName{size:mem::size_of::<OpenFileName>() as u32,owner:ptr::null_mut(),instance:ptr::null_mut(),
        filter:filter.as_ptr(),custom_filter:ptr::null_mut(),max_custom_filter:0,filter_index:1,
        file:file.as_mut_ptr(),max_file:file.len() as u32,file_title:ptr::null_mut(),max_file_title:0,
        initial_dir:ptr::null(),title:ptr::null(),flags:0x00080000|0x00001000|0x00000800,
        file_offset:0,file_extension:0,def_ext:ptr::null(),cust_data:0,hook:ptr::null_mut(),
        template_name:ptr::null(),reserved:ptr::null_mut(),reserved_dword:0,flags_ex:0};
    unsafe {
        if GetOpenFileNameW(&mut info)==0{return None}
    }
    let len=file.iter().position(|v|*v==0).unwrap_or(0);
    let path=String::from_utf16_lossy(&file[..len]);
    if path.is_empty(){None}else{Some(path)}
}

#[cfg(windows)]
fn startup(enabled: bool) -> Result<(),String> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    let exe=production::Config::base_dir().join("Print-Agent.exe");
    if enabled && !exe.is_file(){return Err("Thiếu Print-Agent.exe trong thư mục ứng dụng".into())}
    let key=r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let status=if enabled {
        Command::new("reg.exe").args(["add",key,"/v","FujifilmPrintRust","/t","REG_SZ","/d",&format!("\"{}\"",exe.display()),"/f"]).creation_flags(0x08000000).status()
    }else{
        Command::new("reg.exe").args(["delete",key,"/v","FujifilmPrintRust","/f"]).creation_flags(0x08000000).status()
    };
    match status{Ok(s) if s.success()=>Ok(()),Ok(_) if !enabled=>Ok(()),Ok(s)=>Err(format!("Không lưu được Startup: {:?}",s.code())),Err(e)=>Err(e.to_string())}
}

#[cfg(windows)]
fn startup_checked() -> bool {
    // Đọc Registry trực tiếp, không chạy reg.exe đồng bộ mỗi lần mở cửa sổ.
    use std::{ffi::c_void,ptr};
    #[link(name="advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(root:*mut c_void, sub:*const u16, options:u32,access:u32,key:*mut *mut c_void)->i32;
        fn RegQueryValueExW(key:*mut c_void,value:*const u16,reserved:*mut u32,kind:*mut u32,
           data:*mut u8,size:*mut u32)->i32;
        fn RegCloseKey(key:*mut c_void)->i32;
    }
    fn wide(v:&str)->Vec<u16>{v.encode_utf16().chain(std::iter::once(0)).collect()}
    let mut handle=ptr::null_mut();
    unsafe {
        let root=0x80000001usize as *mut c_void; // HKEY_CURRENT_USER
        if RegOpenKeyExW(root,wide(r"Software\Microsoft\Windows\CurrentVersion\Run").as_ptr(),0,0x20019,&mut handle)!=0{return false}
        let mut size=0u32;let mut kind=0u32;
        let ok=RegQueryValueExW(handle,wide("FujifilmPrintRust").as_ptr(),ptr::null_mut(),&mut kind,ptr::null_mut(),&mut size)==0;
        let _=RegCloseKey(handle);
        ok && size>0
    }
}

#[cfg(windows)]
fn test_connection(url_input:&str, token_input:&str) -> Result<String,String> {
    use std::time::Duration;
    let url=url_input.trim().trim_end_matches('/');
    let token=token_input.trim();
    if url.is_empty(){return Err("Chưa nhập URL Machine-Control.".into())}
    if token.is_empty(){return Err("Chưa nhập Agent Token.".into())}
    let parsed=url::Url::parse(url).map_err(|_|"URL không hợp lệ.".to_string())?;
    let is_local=matches!(parsed.host_str(),Some("localhost"|"127.0.0.1"));
    if parsed.scheme()!="https" && !(is_local && parsed.scheme()=="http") {
        return Err("Chỉ chấp nhận HTTPS; HTTP chỉ được dùng với localhost.".into())
    }
    if parsed.host_str().is_none()||parsed.username()!=""||parsed.password().is_some()
       ||parsed.query().is_some()||parsed.fragment().is_some() {
        return Err("URL phải là địa chỉ API gốc, không kèm tài khoản, query hay fragment.".into())
    }
    let agent_id=std::fs::read_to_string(production::Config::base_dir().join("agent-id.txt"))
       .ok().and_then(|v|uuid::Uuid::parse_str(v.trim()).ok())
       .unwrap_or_else(uuid::Uuid::new_v4).to_string();
    // GET chỉ đọc cấu hình chờ xử lý; không lấy job, không báo kết quả, không in.
    let target=format!("{url}/api/print-agent/config/command");
    let client=ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
    let result=client.get(&target)
       .set("Authorization",&format!("Bearer {token}"))
       .set("X-Print-Agent-Id",&agent_id)
       .set("Accept","application/json")
       .call();
    match result {
        Ok(resp) if (200..300).contains(&resp.status()) => Ok(format!(
            "Kết nối thành công! HTTP {}. URL và Token đã được máy chủ chấp nhận. Không gửi lệnh in.", resp.status())),
        Ok(resp) => Err(format!("Máy chủ trả HTTP {}. Chưa xác minh được Token.",resp.status())),
        Err(ureq::Error::Status(401,_)) => Err("HTTP 401: Agent Token không đúng, hết hiệu lực hoặc không được chấp nhận.".into()),
        Err(ureq::Error::Status(403,_)) => Err("HTTP 403: Token hoặc Agent ID không có quyền truy cập API.".into()),
        Err(ureq::Error::Status(404,_)) => Err("HTTP 404: URL kết nối được nhưng không tìm thấy API Print Agent. Kiểm tra URL gốc.".into()),
        Err(ureq::Error::Status(code,_)) => Err(format!("Máy chủ trả HTTP {code}. Không thể xác nhận kết nối. ")),
        Err(ureq::Error::Transport(e)) => Err(format!("Lỗi mạng, DNS, HTTPS hoặc timeout: {e}")),
    }
}

#[cfg(windows)]
fn read_update_progress() -> Option<(u32,String)> {
    let p=production::Config::base_dir().join("rust-update-progress.json");
    let bytes=std::fs::read(p).ok()?;
    let v:serde_json::Value=serde_json::from_slice(&bytes).ok()?;
    let percent=v["percent"].as_u64()?.min(100) as u32;
    let stage=v["stage"].as_str().unwrap_or("");
    let msg=v["message"].as_str().unwrap_or("Đang kiểm tra cập nhật");
    // 100% only when download or installation really reached that point.
    let status=if stage=="error"{format!("Lỗi cập nhật: {msg}")}else{msg.to_string()};
    Some((percent,format!("{status} — {percent}%")))
}

#[cfg(windows)]
fn main() {
    use native_windows_gui as nwg;
    use production::Config;
    use std::{rc::Rc, sync::mpsc};

    nwg::init().expect("Không thể mở Print Settings");
    nwg::Font::set_global_family("Segoe UI").unwrap();

    let app_version=option_env!("FUJIFILM_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    let embedded=nwg::EmbedResource::load(None).expect("Không tải được icon Settings");
    let gear_icon=embedded.icon(1,Some((32,32))).unwrap_or_else(||nwg::Icon::from_system(nwg::OemIcon::Information));
    let mut window=nwg::Window::default();
    nwg::Window::builder().title(&format!("Fujifilm Print Settings Rust v{app_version} / 印刷設定")).icon(Some(&gear_icon))
        .size((870,785)).position((150,60)).build(&mut window).unwrap();
    let c=Config::load(&Config::base_dir().join("config.ini")).unwrap_or_default();

    let fields=[
        ("API Machine-Control",c.api_base_url.clone()),
        ("Agent Token",c.agent_token.clone()),
        ("Tên hiển thị trên web / Web表示名",c.printer_display_name.clone()),
        ("File mẫu XLSX / テンプレート",c.template_path.clone()),
        ("Chu kỳ kiểm tra (giây) / 確認間隔",c.poll_seconds.to_string()),
        ("Thư mục PDF / PDF保存先",c.pdf_output_path.clone()),
        ("SumatraPDF.exe / PDF印刷ツール",c.pdf_printer_exe.clone()),
        ("Số trang tối đa mỗi PDF / 最大ページ数",c.max_pdf_pages.to_string()),
    ];
    let mut labels=Vec::new();let mut inputs=Vec::new();
    let positions=[12,58,150,196,242,334,380,426];
    for (i,(label,value)) in fields.iter().enumerate(){
        let y=positions[i];
        let mut l=nwg::Label::default();
        nwg::Label::builder().text(label).parent(&window).position((20,y)).size((320,28)).build(&mut l).unwrap();
        let mut t=nwg::TextInput::default();
        nwg::TextInput::builder().text(value).parent(&window).position((350,y)).size((380,28)).build(&mut t).unwrap();
        labels.push(l);inputs.push(t);
    }
    let mut p_label=nwg::Label::default();
    nwg::Label::builder().text("Máy in Windows / Windowsプリンター").parent(&window).position((20,104)).size((320,28)).build(&mut p_label).unwrap();
    let mut printers=vec!["(Máy in mặc định Windows)".to_string()];
    // Chỉ dùng máy in đã lưu để cửa sổ xuất hiện ngay; tải danh sách trên luồng nền.
    if !c.printer_name.is_empty()&&!printers.contains(&c.printer_name){printers.push(c.printer_name.clone());}
    let printer_index=printers.iter().position(|s|s==&c.printer_name).unwrap_or(0);
    let mut printer=nwg::ComboBox::<String>::default();
    nwg::ComboBox::builder().collection(printers).selected_index(Some(printer_index))
       .parent(&window).position((350,104)).size((380,180)).build(&mut printer).unwrap();

    let mut mode_label=nwg::Label::default();
    nwg::Label::builder().text("Chế độ in / 印刷方式").parent(&window).position((20,288)).size((320,28)).build(&mut mode_label).unwrap();
    let mut mode=nwg::ComboBox::<String>::default();
    nwg::ComboBox::builder()
      .collection(vec!["Xuất PDF rồi in / PDFを印刷".to_string(),"Chỉ lưu PDF / PDF保存のみ".to_string()])
      .selected_index(Some(if c.print_mode=="pdf"{1}else{0}))
      .parent(&window).position((350,288)).size((380,100)).build(&mut mode).unwrap();

    let mut browse_xlsx=nwg::Button::default();let mut browse_folder=nwg::Button::default();let mut browse_exe=nwg::Button::default();
    let mut refresh=nwg::Button::default();let mut test=nwg::Button::default();
    nwg::Button::builder().text("Test URL + Token").parent(&window).position((741,58)).size((108,28)).build(&mut test).unwrap();
    nwg::Button::builder().text("Làm mới / 更新").parent(&window).position((741,104)).size((108,28)).build(&mut refresh).unwrap();
    nwg::Button::builder().text("Chọn file...").parent(&window).position((741,196)).size((108,28)).build(&mut browse_xlsx).unwrap();
    nwg::Button::builder().text("Chọn thư mục...").parent(&window).position((741,334)).size((108,28)).build(&mut browse_folder).unwrap();
    nwg::Button::builder().text("Chọn EXE...").parent(&window).position((741,380)).size((108,28)).build(&mut browse_exe).unwrap();

    let mut auto=nwg::CheckBox::default(); let mut boot=nwg::CheckBox::default();
    nwg::CheckBox::builder().text("Tự động kiểm tra cập nhật khi rảnh / 待機中に自動更新").parent(&window)
        .position((20,486)).size((550,32))
        .check_state(if c.auto_update_enabled{nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut auto).unwrap();
    nwg::CheckBox::builder().text("Khởi động cùng Windows khi đăng nhập / Windows起動").parent(&window)
        .position((20,527)).size((550,32))
        .check_state(if startup_checked(){nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut boot).unwrap();

    let mut note=nwg::Label::default();
    nwg::Label::builder().text(&format!("Phiên bản Settings: v{app_version} | Lưu config.ini | Excel 2013 cần thiết để xuất PDF."))
        .parent(&window).position((20,563)).size((820,28)).build(&mut note).unwrap();

    // Settings hiển thị thông tin updater thực tế, không mô phỏng phần trăm chạy.
    let mut update_label=nwg::Label::default();
    nwg::Label::builder().text("Cập nhật Rust / 更新状況: Chưa có hoạt động cập nhật")
        .parent(&window).position((20,598)).size((820,26)).build(&mut update_label).unwrap();
    let mut update_bar=nwg::ProgressBar::default();
    nwg::ProgressBar::builder().range(0..100).pos(0)
        .parent(&window).position((20,630)).size((820,22)).build(&mut update_bar).unwrap();
    if let Some((percent,status))=read_update_progress(){
        update_bar.set_pos(percent);
        update_label.set_text(&format!("Cập nhật Rust / 更新状況: {status}"));
    }
    let mut update_timer=nwg::Timer::default();
    nwg::Timer::builder().parent(&window).interval(750).stopped(false).build(&mut update_timer).unwrap();
    let update_timer_handle=update_timer.handle;
    let mut save=nwg::Button::default();let mut close=nwg::Button::default();
    nwg::Button::builder().text("Lưu cấu hình / 設定を保存").parent(&window).position((465,690)).size((205,42)).build(&mut save).unwrap();
    nwg::Button::builder().text("Đóng / 閉じる").parent(&window).position((692,690)).size((150,42)).build(&mut close).unwrap();

    // Spooler có thể chặn rất lâu nếu có máy in mạng offline.
    // Tuyệt đối không gọi EnumPrintersW trên luồng giao diện.
    let mut printer_notice=nwg::Notice::default();
    nwg::Notice::builder().parent(&window).build(&mut printer_notice).unwrap();
    let notice_handle=printer_notice.handle;
    let notice_sender=printer_notice.sender();
    let (printer_tx,printer_rx)=mpsc::channel::<Vec<String>>();
    let printer_refresh_tx=printer_tx.clone();
    std::thread::spawn(move||{
        let found=installed_printers();
        if printer_tx.send(found).is_ok(){notice_sender.notice();}
    });
    let inputs=Rc::new(inputs);
    let p=Rc::new(printer);let m=Rc::new(mode);
    let a=Rc::new(auto);let b=Rc::new(boot);
    let update_label=Rc::new(update_label);
    let update_bar=Rc::new(update_bar);
    let handles=(save.handle,close.handle,refresh.handle,browse_xlsx.handle,browse_folder.handle,browse_exe.handle,test.handle);
    let win=window.handle;
    let callback=nwg::full_bind_event_handler(&win,move|ev,_data,h|{
        if ev==nwg::Event::OnTimerTick && h==update_timer_handle {
            if let Some((percent,status))=read_update_progress(){
                if update_bar.pos()!=percent{update_bar.set_pos(percent);}
                let value=format!("Cập nhật Rust / 更新状況: {status}");
                if update_label.text()!=value{update_label.set_text(&value);}
            }
            return;
        }
        if ev==nwg::Event::OnNotice && h==notice_handle {
            if let Ok(found)=printer_rx.try_recv(){
                let selected=p.selection_string().unwrap_or_default();
                let mut options=vec!["(Máy in mặc định Windows)".to_string()];
                options.extend(found);
                if !selected.is_empty()&&!options.contains(&selected){options.push(selected.clone());}
                p.set_collection(options);
                p.set_selection_string(&selected);
            }
            return;
        }
        if ev==nwg::Event::OnWindowClose&&h==win {nwg::stop_thread_dispatch();return}
        if ev!=nwg::Event::OnButtonClick {return}
        if h==handles.1 {nwg::stop_thread_dispatch();return}
        if h==handles.6 {
            // Dùng nội dung đang nhập; không yêu cầu lưu token xuống ổ đĩa.
            let result=test_connection(&inputs[0].text(), &inputs[1].text());
            match result {
                Ok(msg)=>{nwg::simple_message("Kết nối Cloudflare thành công / 接続成功",&msg);}
                Err(msg)=>{nwg::error_message("Kiểm tra URL / Token thất bại",&msg);}
            }
        } else if h==handles.2 {
            // Làm mới từ luồng nền; không đóng cửa sổ và không mất các trường đang nhập.
            let sender=printer_notice.sender();
            let tx=printer_refresh_tx.clone();
            std::thread::spawn(move||{
                if tx.send(installed_printers()).is_ok(){sender.notice();}
            });
        } else if h==handles.3 {if let Some(s)=pick_file("xlsx"){inputs[3].set_text(&s);}}
        else if h==handles.4 {if let Some(s)=pick_file("folder"){inputs[5].set_text(&s);}}
        else if h==handles.5 {if let Some(s)=pick_file("exe"){inputs[6].set_text(&s);}}
        else if h==handles.0 {
            let parse=||->Result<Config,String>{
                let poll_seconds=inputs[4].text().parse::<u64>().map_err(|_|"Chu kỳ phải là số")?;
                let max_pdf_pages=inputs[7].text().parse::<usize>().map_err(|_|"Số trang phải là số")?;
                let printer_name=if p.selection_string().as_deref()==Some("(Máy in mặc định Windows)"){String::new()}else{p.selection_string().unwrap_or_default()};
                let c=Config{
                    api_base_url:inputs[0].text().trim().into(),agent_token:inputs[1].text().trim().into(),
                    printer_name,printer_display_name:inputs[2].text().trim().into(),
                    template_path:inputs[3].text().trim().into(),poll_seconds,max_pdf_pages,
                    pdf_output_path:inputs[5].text().trim().into(),pdf_printer_exe:inputs[6].text().trim().into(),
                    print_mode:if m.selection_string().as_deref()==Some("Chỉ lưu PDF / PDF保存のみ"){"pdf".into()}else{"pdfprint".into()},
                    auto_update_enabled:a.check_state()==nwg::CheckBoxState::Checked,
                };
                c.check()?;
                if !c.template_path.to_ascii_lowercase().ends_with(".xlsx"){return Err("Mẫu phải là file .xlsx".into())}
                if !c.pdf_output_path.is_empty()&&!std::path::Path::new(&c.pdf_output_path).is_absolute(){return Err("Thư mục PDF phải là đường dẫn tuyệt đối".into())}
                if c.print_mode=="pdfprint"&&!c.pdf_printer_exe.to_ascii_lowercase().ends_with(".exe"){return Err("Bộ in PDF cần file .exe".into())}
                Ok(c)
            };
            let result=parse().and_then(|c|c.save(&Config::base_dir().join("config.ini")))
                .and_then(|_|startup(b.check_state()==nwg::CheckBoxState::Checked));
            match result{
                Ok(())=>{nwg::simple_message("Fujifilm Print","Đã lưu config.ini và cài đặt khởi động cùng Windows. Hãy khởi động lại Agent.");}
                Err(e)=>{nwg::error_message("Không lưu được cấu hình",&e);}
            }
        }
    });
    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&callback);
}
#[cfg(not(windows))]
fn main(){eprintln!("Print Settings Rust chỉ chạy trên Windows");}
