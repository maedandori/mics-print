// Tạo icon ICO trực tiếp lúc build, không phụ thuộc file hình ảnh bên ngoài.
#[cfg(windows)]
fn main() {
    let agent=std::env::var_os("CARGO_FEATURE_AGENT_ICON").is_some();
    let settings=std::env::var_os("CARGO_FEATURE_SETTINGS_ICON").is_some();
    if agent && settings {panic!("Chọn đúng một loại icon khi build từng EXE");}
    if !agent && !settings {return}
    let output=std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let file=output.join(if agent{"print-agent.ico"}else{"print-settings.ico"});
    std::fs::write(&file,build_icon(agent)).expect("Không ghi được ICO");
    let path=file.to_str().expect("Path ICO");
    winres::WindowsResource::new()
        .set_icon(path)
        .set("FileDescription",if agent{"Fujifilm Print Agent"}else{"Fujifilm Print Settings"})
        .set("ProductName","Fujifilm Print Rust")
        .compile().expect("Không nhúng được icon vào EXE");
}
#[cfg(not(windows))]
fn main() {}
#[cfg(windows)]
fn build_icon(printer:bool)->Vec<u8>{
    let mut entries=Vec::new();
    for size in [16usize,32,48] {entries.push((size,ico_bitmap(size,printer)));}
    let mut result=Vec::new();
    result.extend_from_slice(&0u16.to_le_bytes());
    result.extend_from_slice(&1u16.to_le_bytes());
    result.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset=6+entries.len()*16;
    for (size,entry) in &entries {
        result.push(*size as u8);result.push(*size as u8);
        result.push(0);result.push(0);
        result.extend_from_slice(&1u16.to_le_bytes());
        result.extend_from_slice(&32u16.to_le_bytes());
        result.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        result.extend_from_slice(&(offset as u32).to_le_bytes());
        offset+=entry.len();
    }
    for (_,v) in entries {result.extend_from_slice(&v);}
    result
}
#[cfg(windows)]
fn ico_bitmap(n:usize,printer:bool)->Vec<u8>{
    let mut p=vec![[0u8;4];n*n];
    let navy=[33,54,82,255];
    let dark=[23,39,58,255];
    let blue=[49,124,207,255];
    let white=[245,249,255,255];
    let gray=[175,192,210,255];
    fn rect(p:&mut [[u8;4]],n:usize,x1:f32,y1:f32,x2:f32,y2:f32,color:[u8;4]){
        for y in 0..n{for x in 0..n{
            let xx=(x as f32+0.5)/n as f32;let yy=(y as f32+0.5)/n as f32;
            if xx>=x1 && xx<x2 && yy>=y1 && yy<y2 {p[y*n+x]=color;}
        }}
    }
    if printer {
        // Biểu tượng máy in: giấy trắng, thân máy xanh, khe giấy và đèn báo.
        rect(&mut p,n,0.22,0.07,0.78,0.50,white);
        rect(&mut p,n,0.27,0.15,0.73,0.19,gray);
        rect(&mut p,n,0.27,0.25,0.69,0.29,gray);
        rect(&mut p,n,0.10,0.32,0.90,0.77,dark);
        rect(&mut p,n,0.14,0.37,0.86,0.73,blue);
        rect(&mut p,n,0.70,0.43,0.77,0.50,[76,224,159,255]);
        rect(&mut p,n,0.22,0.60,0.78,0.91,navy);
        rect(&mut p,n,0.27,0.65,0.73,0.86,white);
        rect(&mut p,n,0.33,0.72,0.67,0.76,gray);
    }else{
        // Bánh răng 8 răng: răng, vành tròn và tâm trống rõ ở kích cỡ 16px.
        let center=(n as f32-1.)/2.;
        for y in 0..n{for x in 0..n{
            let dx=x as f32-center;let dy=y as f32-center;
            let rr=(dx*dx+dy*dy).sqrt()/(n as f32);
            let angle=dy.atan2(dx);
            let eight=(angle*4.0).cos().abs();
            let in_gear=rr<0.33 || (rr<0.44 && eight>0.73);
            if in_gear && rr>0.135 {p[y*n+x]=if rr<0.31{blue}else{navy};}
            if rr<=0.135 {p[y*n+x]=white;}
        }}
    }
    let mut out=Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(n as i32).to_le_bytes());
    out.extend_from_slice(&((n*2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((n*n*4) as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());out.extend_from_slice(&0u32.to_le_bytes());
    for y in (0..n).rev(){for x in 0..n{
        let [r,g,b,a]=p[y*n+x];out.extend_from_slice(&[b,g,r,a]);
    }}
    let and_stride=((n+31)/32)*4;
    for y in (0..n).rev(){
        let mut bits=vec![0u8;and_stride];
        for x in 0..n{if p[y*n+x][3]==0{bits[x/8]|=0x80>>(x%8);}}
        out.extend_from_slice(&bits);
    }
    out
}
