fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = nextion_tft_toolkit::zi::FontFile::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let g = f.find(a[2].chars().next().unwrap() as u16).unwrap();
    for row in g.unpack(f.line_px).unwrap().chunks(g.cell_w()) {
        println!("{}", row.iter().map(|&p| b" .:-=+*#"[p as usize] as char).collect::<String>());
    }
}
