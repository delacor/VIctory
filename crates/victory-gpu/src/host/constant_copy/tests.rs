use super::*;
use std::collections::BTreeSet;

#[test]
#[ignore = "isolated hardware process with VICTORY_GPU_CONSTANTS=1 and two-sided buffers"]
fn gpu_constants_keep_gpu_writes_cpu_changes_ordering_and_fenced_lifetimes() {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc,
    };
    assert!(on(), "set VICTORY_GPU_CONSTANTS=1");
    let reference = std::env::var_os("GPU_CONSTANT_REFERENCE").is_some();
    let host = host().expect("hardware D3D12 host");
    const SIDE: usize = 1 << 16;
    const LEN: usize = 512;
    const ROUNDS: usize = 8;
    // This isolated process owns the mapping through its last completed
    // submission. Leave it mapped until process exit, as the relay fixtures do.
    let base = unsafe {
        VirtualAlloc(std::ptr::null(), SIDE * 2, MEM_RESERVE | MEM_COMMIT | 0x20_0000, PAGE_READWRITE)
    }.cast::<u8>();
    assert!(!base.is_null());
    let src = base as usize;
    let dst = src + SIDE;
    crate::shadow::register(0x1cc1, src as u64, SIDE as u64);
    crate::shadow::register(0x1cc2, dst as u64, SIDE as u64);
    let source = crate::shadow::find(src as u64, SIDE as u64).unwrap();
    let destination = crate::shadow::find(dst as u64, SIDE as u64).unwrap();
    assert!(source.card_sides().is_some() && destination.card_sides().is_some());
    let a = host.new_list().unwrap();
    let b = host.new_list().unwrap();
    let readable = D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER
        | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
        | D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
    for round in 0..ROUNDS {
        if round != 0 { a.reset(); b.reset(); }
        assert_eq!(b.deferred().gpu_constants, 0);
        let plain = 0x20 + round as u8;
        let gpu_value = 0x90 + round as u8;
        let cpu_value = 0x60 + round as u8;
        let conflict_value = 0xe0 + round as u8;
        // SAFETY: both complete title allocations are ours.
        unsafe {
            base.write_bytes(plain, SIDE);
            base.add(SIDE).write_bytes(0x11, SIDE);
        }
        let (upload, mapped) = host.upload_buffer(4096).unwrap();
        unsafe { mapped.write_bytes(gpu_value, 300) };
        source.refresh(&a, src as u64, SIDE as u64);
        source.gpu_writes(&a, (src + 64) as u64, 300);
        unsafe {
            a.list.CopyBufferRegion(&source.buffer, 64, &upload, 0, 300);
            a.list.ResourceBarrier(&[transition(
                &source.buffer, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COMMON,
            )]);
        }
        a.keep(&upload);
        a.close();
        // Separate two valid cases: asynchronous nonoverlapping CPU
        // updates, and an overwrite after the complete GPU flight/merge.
        // Native queue completion alone does not finish VIctory's landing.
        let ordered_conflict = round % 2 == 1;
        if ordered_conflict { host.execute(&[&a]); }
        else { host.execute_own(&a); }
        drop(upload); // the submitted list retains it
        // The CPU changes bytes beside and after those the GPU wrote.
        unsafe {
            base.add(450).write_bytes(cpu_value, 32);
            // The CPU wins these bytes when both sides changed them.
            if ordered_conflict { base.add(160).write_bytes(conflict_value, 16); }
        };
        assert!(b.place_copy(dst + 96, src + 32, 640));
        assert!(recent_source(dst + 112, LEN));
        assert!(!recent_source(src + 16, 240));
        let back = host.read_back_buffer((MOST * LEN) as u64).unwrap();
        let mut addresses = BTreeSet::new();
        for i in 0..MOST {
            let (buffer, gpu) = if reference {
                let (buffer, gpu, mapped) = b.scratch_held(LEN as u64).unwrap();
                b.defer_fill_buffer(&buffer, mapped, (dst + 112) as *const u8, LEN);
                (buffer, gpu)
            } else {
                b.constant_from_gpu((dst + 112) as u64, LEN as u64)
                    .expect("recent placed bytes should use the GPU constant path")
            };
            assert_eq!(gpu % 256, 0);
            assert!(addresses.insert(gpu), "each recorded copy owns a disjoint buffer");
            unsafe {
                if !reference { b.list.ResourceBarrier(&[transition(&buffer, readable, D3D12_RESOURCE_STATE_COPY_SOURCE)]); }
                let offset = gpu - buffer.GetGPUVirtualAddress();
                b.list.CopyBufferRegion(&back, (i * LEN) as u64, &buffer, offset, LEN as u64);
            }
            drop(buffer); // keep() must preserve it until completion
        }
        if !reference {
        assert!(b.constant_from_gpu((dst + 112) as u64, LEN as u64).is_none(),
            "the per-recording allocation cap must preserve the CPU fallback");
        assert!(b.deferred().copies.iter().all(|step| !matches!(step, Step::Fill(..))),
            "the copy chain must add no CPU fill that reads the placed destination");
        }
        b.close();
        let number = host.execute_own(&b);
        settle_now(WHY_SYNC);
        assert!(FINISHED.load(std::sync::atomic::Ordering::Acquire) >= number);
        let expected: Vec<_> = (48..48 + LEN).map(|i| match i {
            160..176 if ordered_conflict => conflict_value,
            64..364 => gpu_value,
            450..482 => cpu_value,
            _ => plain,
        }).collect();
        unsafe {
            let mut data = std::ptr::null_mut();
            back.Map(0, None, Some(&mut data)).unwrap();
            let actual = std::slice::from_raw_parts(data.cast::<u8>(), MOST * LEN);
            for i in 0..MOST {
                assert_eq!(&actual[i * LEN..(i + 1) * LEN], expected.as_slice(),
                    "GPU writes and CPU changes, round {round} copy {i}");
            }
            back.Unmap(0, None);
            host.device.GetDeviceRemovedReason().unwrap();
        }
    }
    a.reset(); b.reset();
    assert_eq!(b.deferred().gpu_constants, 0);
    eprintln!("reference_cpu_fill={reference}");
    eprintln!("GPU_CONSTANT_TEST 32 disjoint copies, GPU writes plus async CPU changes and ordered conflicts, exact readback, caller release and fenced resets");
}
