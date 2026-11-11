#![cfg_attr(windows, windows_subsystem="windows")]
#[path = "../security/credentials.rs"] mod credentials;
#[cfg(windows)]
fn main(){
    use native_windows_gui as nwg;
    nwg::init().expect("Không thể khởi tạo Windows GUI");
    nwg::Font::set_global_family("Segoe UI").ok();
    let mut window=nwg::Window::default();
    nwg::Window::builder().title("MICS Credentials — Bảo mật thông tin")
        .size((640,330)).position((200,150)).build(&mut window).unwrap();
    let mut db_label=nwg::Label::default();
    nwg::Label::builder().text("Mật khẩu Firebird").parent(&window).position((20,42)).size((210,28)).build(&mut db_label).unwrap();
    let mut db=nwg::TextInput::default();
    nwg::TextInput::builder().password(Some('*')).parent(&window).position((230,42)).size((375,28)).build(&mut db).unwrap();
    let mut token_label=nwg::Label::default();
    nwg::Label::builder().text("MICS_SYNC_TOKEN").parent(&window).position((20,100)).size((210,28)).build(&mut token_label).unwrap();
    let mut token=nwg::TextInput::default();
    nwg::TextInput::builder().password(Some('*')).parent(&window).position((230,100)).size((375,28)).build(&mut token).unwrap();
    let mut note=nwg::Label::default();
    nwg::Label::builder().text("Chỉ lưu dữ liệu đã mã hóa DPAPI trong LOCALAPPDATA. Không hiển thị mật khẩu cũ.")
      .parent(&window).position((20,154)).size((600,48)).build(&mut note).unwrap();
    let mut save=nwg::Button::default();
    nwg::Button::builder().text("Lưu bảo mật / 安全に保存").parent(&window).position((245,230)).size((220,45)).build(&mut save).unwrap();
    let mut close=nwg::Button::default();
    nwg::Button::builder().text("Đóng").parent(&window).position((480,230)).size((125,45)).build(&mut close).unwrap();
    let handle=window.handle;
    let callback=nwg::full_bind_event_handler(&handle,move|event,_data,h|{
        if event==nwg::Event::OnWindowClose&&h==handle{nwg::stop_thread_dispatch();return}
        if event!=nwg::Event::OnButtonClick{return}
        if h==close.handle{nwg::stop_thread_dispatch();return}
        if h==save.handle{
            let record=credentials::Credentials{db_password:db.text(),mics_sync_token:token.text()};
            match credentials::save(&record){
                Ok(())=>{
                    db.set_text("");token.set_text("");
                    nwg::simple_message("MICS Credentials","Đã lưu dưới dạng mã hóa DPAPI. Hãy khởi động lại Print Agent.");
                }
                Err(e)=>nwg::error_message("Không thể lưu credentials",&e)
            }
        }
    });
    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&callback);
}
#[cfg(not(windows))]
fn main(){eprintln!("Chỉ hỗ trợ Windows");}
