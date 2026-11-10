use super::{Config,log};
use std::{sync::mpsc,thread,time::Duration};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;

pub fn start(c:&Config,agent_id:String)->mpsc::Receiver<()>{
 let (sender,receiver)=mpsc::channel();
 let url=c.api_base_url.trim_end_matches('/').to_string();
 let token=c.agent_token.clone();
 thread::spawn(move||{
  let endpoint=if url.starts_with("https://"){url.replacen("https://","wss://",1)}
               else{url.replacen("http://","ws://",1)};
  let target=format!("{endpoint}/api/print-agent/socket");
  loop {
   let result=(||->Result<(),String>{
    let mut request=target.as_str().into_client_request().map_err(|e|e.to_string())?;
    request.headers_mut().insert("Authorization",HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e|e.to_string())?);
    request.headers_mut().insert("X-Print-Agent-Id",HeaderValue::from_str(&agent_id).map_err(|e|e.to_string())?);
    let (mut socket,_) = tungstenite::connect(request).map_err(|e|e.to_string())?;
    log("WebSocket Rust đã kết nối");
    let _=sender.send(());
    loop{
     match socket.read(){
      Ok(message) => {
       if message.is_text() || message.is_binary(){let _=sender.send(());}
       else if message.is_ping(){let _=socket.send(tungstenite::Message::Pong(message.into_data()));}
       else if message.is_close(){break}
      }
      Err(e)=>return Err(e.to_string())
     }
    }
    Ok(())
   })();
   if let Err(e)=result{log(&format!("WebSocket mất kết nối, dùng polling dự phòng: {e}"))}
   thread::sleep(Duration::from_secs(5));
  }
 });
 receiver
}
