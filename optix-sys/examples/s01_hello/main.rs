use cuda_sys::{
    cudaFree, cudaGetDeviceCount, CUcontext, CUdeviceptr, CUstream,
};
use optix_sys::{optixInit, OptixResult_OPTIX_SUCCESS};

fn main() {
    unsafe {
        cudaFree(std::ptr::null_mut());
        let mut num_devices = 0i32;
        cudaGetDeviceCount(&mut num_devices as *mut i32);
        if num_devices == 0 {
            panic!("No CUDA devices found");
        }
        println!("Found {} CUDA devices", num_devices);

        let result = optixInit();
        if result != OptixResult_OPTIX_SUCCESS {
            panic!("OptiX init failed!");
        }

        println!("OptiX initialized successfully! Yay!");
    }
}