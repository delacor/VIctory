//! Inspect the emitted native pixel program, rather than a local Boolean.
use super::*;
use victory_shader::native::{HwStage, NativeProgram, StageIo};
use victory_shader::{dxbc::Container, io, ir, resource, structure};

fn disassemble(bytes: &[u8]) -> windows::core::Result<String> {
    type Create = unsafe extern "system" fn(
        *const GUID, *const GUID, *mut *mut std::ffi::c_void,
    ) -> windows::core::HRESULT;
    // SAFETY: the documented compiler interface and a live buffer of ours.
    unsafe {
        let create: Create = std::mem::transmute(
            GetProcAddress(library()?, s!("DxcCreateInstance"))
                .ok_or_else(windows::core::Error::from_thread)?,
        );
        let mut compiler = std::ptr::null_mut();
        create(&CLSID_DxcCompiler, &IDxcCompiler3::IID, &mut compiler).ok()?;
        let compiler = IDxcCompiler3::from_raw(compiler);
        let buffer = DxcBuffer { Ptr: bytes.as_ptr().cast(), Size: bytes.len(), Encoding: 0 };
        let result: IDxcResult = compiler.Disassemble(&buffer)?;
        result.GetStatus()?.ok()?;
        let blob = result.GetResult()?;
        let text = std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
        Ok(String::from_utf8_lossy(text).into_owned())
    }
}

fn shader_flags(text: &str) -> u64 {
    let named = text.lines().find(|l| l.starts_with("!dx.entryPoints = ")).expect("entryPoints");
    let id = named.split_once("!{").unwrap().1.trim_end_matches('}').trim();
    let entry = text.lines().find(|l| l.starts_with(&format!("{id} = "))).expect("entry");
    let props = entry.rsplit(',').next().unwrap().trim().trim_end_matches('}');
    if props == "null" { return 0; }
    let props = text.lines().find(|l| l.starts_with(&format!("{props} = "))).expect("properties");
    let Some((_, flags)) = props.split_once("i64 ") else { return 0 };
    flags.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap()
}

#[test]
fn native_early_depth_survives_translation_and_depth_exports_remain_late() {
    let Ok(carrier) = dxc("float4 main() : SV_Target { return float4(1,0,0,1); }", "main", "ps_6_0") else {
        eprintln!("DXC unavailable; early-depth metadata test skipped");
        return;
    };
    for (control, expected) in [(0, false), (1 << 12, true), (1 << 12 | 1, false)] {
        let depth = control & 1 != 0;
        let depth_carrier;
        let carrier = if depth {
            depth_carrier = dxc(
                "void main(out float4 c : SV_Target, out float z : SV_Depth) { c=float4(1,0,0,1); z=1; }",
                "main", "ps_6_0",
            ).unwrap();
            &depth_carrier
        } else { &carrier };
        let container = Container::parse(carrier).unwrap();
        let mut words = vec![
            0x7e00_02f2u32, 0x7e02_0280, 0x7e04_0280, 0x7e06_02f2,
            if depth { 0xf800_100f } else { 0xf800_180f }, 0x0302_0100,
        ];
        if depth {
            words.extend([0xf800_1881, 0]); // exp mrtz v0, off, off, off done vm
        }
        words.push(0xbf81_0000);
        let native = NativeProgram {
            stage: HwStage::Ps, wave64: false, vgprs: 4, layout: vec![], used: vec![],
            io: StageIo::Ps { inputs: vec![], input_addr: 0, db_shader_control: control, patches: vec![] },
            code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        };
        let program = ir::build(&native.code, native.wave64, native.vgprs).unwrap();
        let structured = structure::structure(&program).unwrap();
        let resources = resource::track(&program, &native.layout, native.stage, Some(&structured)).unwrap();
        let raised = io::raise(&program, &native).unwrap();
        let shader = victory_shader::emit::emit(
            &container, &native, &program, &structured, &raised, &resources,
        ).unwrap();
        if let Some(path) = victory_shader::emit::validator::Validator::default_path() {
            victory_shader::emit::validator::Validator::load(Some(&path)).unwrap()
                .validate(&mut shader.dxil.clone()).unwrap();
        }
        let text = disassemble(&shader.dxil).unwrap();
        let flags = shader_flags(&text);
        assert_eq!(flags & 8 != 0, expected, "DB_SHADER_CONTROL={control:#x}; flags={flags:#x}\n{text}");
        eprintln!("DB_SHADER_CONTROL={control:#x}: DXIL flags={flags:#x}");
    }
}
