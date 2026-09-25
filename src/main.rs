use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_yaml::Value;

use nextion_tft_toolkit::hmi::{self, hex_encode, AttrValue, Selector};
use nextion_tft_toolkit::spec::UiSpec;
use nextion_tft_toolkit::target::Target;
use nextion_tft_toolkit::tft;

#[derive(Parser)]
#[command(
    name = "nxtft",
    version,
    about = "Decode and patch Nextion .HMI/.tft files -- NX8048P050-011R-Y only"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Dump a .HMI's pages/components/attributes to readable YAML.
    HmiDecode { input: PathBuf, output: PathBuf },
    /// Rewrite one or more .HMI attribute values in place (same length only).
    HmiPatch {
        input: PathBuf,
        output: PathBuf,
        /// PAGE:OBJNAME:ATTR=VALUE, repeatable.
        #[arg(long = "set", required = true)]
        set: Vec<String>,
    },
    /// Rewrite a .tft text-pool slot by searching for the current text.
    TftPatchText {
        input: PathBuf,
        output: PathBuf,
        /// OLDTEXT=NEWTEXT, repeatable.
        #[arg(long = "set", required = true)]
        set: Vec<String>,
        #[arg(long, default_value = "NX8048P050-011R-Y")]
        target: Target,
    },
    /// Rewrite a .tft component's x,y,w,h geometry by searching for the current quad.
    TftPatchGeom {
        input: PathBuf,
        output: PathBuf,
        /// X,Y,W,H=NEWX,NEWY,NEWW,NEWH, repeatable.
        #[arg(long = "set", required = true)]
        set: Vec<String>,
        /// Disambiguate a non-unique match by file offset (hex like 0xc0ef4 or decimal).
        #[arg(long)]
        at: Option<String>,
        #[arg(long, default_value = "NX8048P050-011R-Y")]
        target: Target,
    },
    /// Patch a scaffold .tft to match a YAML UI spec, using the scaffold's own .HMI to
    /// resolve each component's current geometry/type/text.
    Compile {
        #[arg(long)]
        scaffold_hmi: PathBuf,
        #[arg(long)]
        scaffold_tft: PathBuf,
        #[arg(long)]
        spec: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Render a YAML UI spec as a static HTML page for previewing a layout
    /// in a browser -- no scaffold .HMI/.tft needed.
    RenderHtml { spec: PathBuf, output: PathBuf },
}

fn parse_offset(s: &str) -> Result<usize, std::num::ParseIntError> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        usize::from_str_radix(hex, 16)
    } else {
        s.parse::<usize>()
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::HmiDecode { input, output } => {
            let decoded = hmi::decode_file(&input)?;
            let readable = to_readable_value(&decoded);
            let yaml = serde_yaml::to_string(&readable)?;
            std::fs::write(&output, yaml)?;
            let n_pages = decoded.pages.len();
            let n_comps: usize = decoded.pages.iter().map(|p| p.components.len()).sum();
            println!(
                "decoded {n_pages} page(s), {n_comps} component(s) -> {}",
                output.display()
            );
        }

        Command::HmiPatch { input, output, set } => {
            let data = std::fs::read(&input)?;
            let decoded = hmi::decode(&data)?;
            let mut data = data;

            for spec_str in &set {
                let (sel, new_value) = Selector::parse(spec_str)?;
                hmi::patch_attr(&mut data, &decoded, &sel, &new_value)?;
                println!("patched {spec_str}");
            }

            std::fs::write(&output, &data)?;
            println!("wrote {}", output.display());
        }

        Command::TftPatchText {
            input,
            output,
            set,
            target,
        } => {
            let mut data = std::fs::read(&input)?;
            tft::parse_header(&data, target)?;

            for spec_str in &set {
                let (old, new) = tft::parse_text_set_spec(spec_str)?;
                let off = tft::patch_text(&mut data, &old, &new)?;
                println!("patched text {old:?} -> {new:?} at {off:#x}");
            }

            std::fs::write(&output, &data)?;
            println!("wrote {}", output.display());
        }

        Command::TftPatchGeom {
            input,
            output,
            set,
            at,
            target,
        } => {
            let mut data = std::fs::read(&input)?;
            tft::parse_header(&data, target)?;
            let at = at.map(|s| parse_offset(&s)).transpose()?;

            for spec_str in &set {
                let (old, new) = tft::parse_geom_set_spec(spec_str)?;
                let off = tft::patch_geom(&mut data, old, new, at)?;
                println!("patched geometry {old:?} -> {new:?} at {off:#x}");
            }

            std::fs::write(&output, &data)?;
            println!("wrote {}", output.display());
        }

        Command::Compile {
            scaffold_hmi,
            scaffold_tft,
            spec,
            output,
        } => {
            let spec = UiSpec::from_file(&spec)?;
            let target = spec.target()?;
            let scaffold_decoded = hmi::decode_file(&scaffold_hmi)?;
            let mut tft_data = std::fs::read(&scaffold_tft)?;

            let changes = nextion_tft_toolkit::spec::compile(
                &spec,
                &scaffold_decoded,
                &mut tft_data,
                target,
            )?;

            for c in &changes {
                println!("{}.{}: {} -> {}", c.objname, c.field, c.from, c.to);
            }
            if changes.is_empty() {
                println!("no changes -- scaffold already matches the spec");
            }

            std::fs::write(&output, &tft_data)?;
            println!("wrote {}", output.display());
        }

        Command::RenderHtml { spec, output } => {
            let spec = UiSpec::from_file(&spec)?;
            let html = nextion_tft_toolkit::html::render(&spec);
            std::fs::write(&output, html)?;
            println!(
                "rendered {} component(s) -> {}",
                spec.components.len(),
                output.display()
            );
        }
    }

    Ok(())
}

/// Builds a `serde_yaml::Value` tree mirroring `hmi_tool.py`'s flattened
/// `to_readable` output: one mapping per page/component with each
/// attribute name as a key, values typed as string/int/sequence-of-bytes
/// depending on [`AttrValue`]. Building a `Value` tree (rather than
/// hand-writing YAML text) means `serde_yaml` handles escaping, so this
/// can't emit the invalid `\u{...}` escapes Rust's own `Debug` format uses.
fn to_readable_value(decoded: &hmi::Decoded) -> Value {
    let mut root = serde_yaml::Mapping::new();
    root.insert(
        Value::from("payload_start"),
        Value::from(decoded.payload_start as u64),
    );

    let pages: Vec<Value> = decoded.pages.iter().map(page_to_value).collect();
    root.insert(Value::from("pages"), Value::Sequence(pages));

    Value::Mapping(root)
}

fn page_to_value(page: &hmi::Page) -> Value {
    let mut map = serde_yaml::Mapping::new();
    map.insert(
        Value::from("objname"),
        match &page.objname {
            Some(s) => Value::from(s.clone()),
            None => Value::Null,
        },
    );
    for attr in &page.attrs {
        map.insert(
            Value::from(attr.name.clone()),
            attr_value_to_value(&attr.value),
        );
    }
    let components: Vec<Value> = page.components.iter().map(component_to_value).collect();
    map.insert(Value::from("components"), Value::Sequence(components));
    Value::Mapping(map)
}

fn component_to_value(comp: &hmi::Component) -> Value {
    let mut map = serde_yaml::Mapping::new();
    map.insert(
        Value::from("_file_offset"),
        Value::from(comp.file_offset as u64),
    );
    for attr in &comp.attrs {
        map.insert(
            Value::from(attr.name.clone()),
            attr_value_to_value(&attr.value),
        );
    }
    Value::Mapping(map)
}

fn attr_value_to_value(value: &AttrValue) -> Value {
    match value {
        AttrValue::Str(s) => Value::from(s.clone()),
        AttrValue::Int(i) => Value::from(*i),
        AttrValue::Blob(b) | AttrValue::Raw(b) | AttrValue::UnknownPad(b) => {
            Value::from(hex_encode(b))
        }
        AttrValue::Other(_, b) => Value::from(hex_encode(b)),
    }
}
