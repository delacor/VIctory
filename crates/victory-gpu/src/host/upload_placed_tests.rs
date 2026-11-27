//! Actual GPU lifetime and readback of a placed list upload.
use super::*;

#[test]
fn pooled_upload_survives_caller_release_until_gpu_completion_and_reset() {
    let host = host().expect("a hardware D3D12 device");
    let list = host.new_list().unwrap();
    let held = crate::pool::view_memory()[1];
    let (source, data) = list.upload_for_steps(host, 4096, true, true).unwrap();
    let placement = crate::pool::view_memory()[1] - held;
    assert!(placement >= 4096, "the placed path must actually be exercised");
    let expected: Vec<u8> = (0..4096).map(|i| (i * 37 + i / 11) as u8).collect();
    // SAFETY: an exclusively owned, mapped upload of 4096 bytes.
    unsafe { std::ptr::copy_nonoverlapping(expected.as_ptr(), data, expected.len()); }
    let back = host.read_back_buffer(4096).unwrap();
    // The recorded command list does not retain resources itself. The list's
    // deferred upload hold must survive dropping the caller's source reference.
    unsafe {
        list.list.ResourceBarrier(&[transition(&source, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
        list.list.CopyBufferRegion(&back, 0, &source, 0, 4096);
    }
    drop(source);
    assert_eq!(crate::pool::view_memory()[1], held + placement);
    let (other, other_data) = crate::pool::view_buffer(host, 4096).unwrap();
    assert_ne!(data, other_data, "the in-flight upload must not be handed out");
    unsafe { std::ptr::write_bytes(other_data, 0xef, 4096); }
    list.close();
    // SAFETY: all objects outlive the submission; the fence proves completion
    // before reset returns the placed allocation.
    unsafe {
        let queue: ID3D12CommandQueue = host.device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default()
        }).unwrap();
        queue.ExecuteCommandLists(&[Some(list.list.cast().unwrap())]);
        let fence: ID3D12Fence = host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
        queue.Signal(&fence, 1).unwrap();
        while fence.GetCompletedValue() < 1 {
            host.device.GetDeviceRemovedReason().unwrap();
            std::thread::yield_now();
        }
        let mut mapped = std::ptr::null_mut();
        back.Map(0, Some(&D3D12_RANGE { Begin: 0, End: 4096 }), Some(&mut mapped)).unwrap();
        let actual = std::slice::from_raw_parts(mapped.cast::<u8>(), 4096).to_vec();
        back.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        assert_eq!(actual, expected);
    }
    drop(other);
    list.reset();
    assert_eq!(crate::pool::view_memory()[1], held);
}
