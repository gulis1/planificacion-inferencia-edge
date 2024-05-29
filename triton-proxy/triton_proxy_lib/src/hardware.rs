use log::{info, error};
use sysinfo::System;
use rustacuda::{device::DeviceAttribute, prelude::*};
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {

    pub cpu_arch: String,
    pub physical_cores: usize,
    pub total_memory: u64,
    pub gpus: Vec<GpuInfo>
}

impl ToString for SystemInfo {
    fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub memory: usize,
    pub core_count: usize
}

pub fn get_hardware_info() -> SystemInfo {
    
    let sys = System::new_all();
    let info = SystemInfo {
        cpu_arch: System::cpu_arch().unwrap_or("Unknown".into()),
        physical_cores: sys.physical_core_count().unwrap_or(0),
        total_memory: sys.total_memory(),
        gpus: get_gpu_info()
    };
    
    info
}

fn get_gpu_info() -> Vec<GpuInfo> {

    let success = rustacuda::init(CudaFlags::empty())
        .map(|_| Device::num_devices().unwrap_or(0));

    match success {
        Err(_) => {
            error!("Failed to init CUDA to query GPU devices.");
            Vec::new()
        },
        Ok(0) => {
            info!("NO GPU devices found.");
            Vec::new() 
        },
        Ok(n_devices) => {
            info!("Found {n_devices} GPUs available.");
            Device::devices().ok().expect("Failed to get CUDA devices.")
                .map(|dev| {
                    let dev = dev.expect("Failed to get GPU details.");
                    let sm_count = dev.get_attribute(DeviceAttribute::MultiprocessorCount).unwrap_or(0) as usize;
                    let major = dev.get_attribute(DeviceAttribute::ComputeCapabilityMajor).expect("Failed to get major.");
                    let minor = dev.get_attribute(DeviceAttribute::ComputeCapabilityMinor).expect("Failed to get minor.");
                    GpuInfo {
                        name: dev.name().unwrap_or("Unknown".into()),
                        memory: dev.total_memory().unwrap_or(0),
                        core_count: sm_count * sm_arch_to_smcores(major, minor)
                    }
                })
                .collect()
        }
    }
}

fn sm_arch_to_smcores(major: i32, minor: i32) -> usize {
    
    // Adaptado de: https://github.com/NVIDIA/cuda-samples/blob/master/Common/helper_cuda.h#L643
    // Defines for GPU Architecture types (using the SM version to determine
    // the # of cores per SM
    let gpu_arch_to_sms: [(i32, usize); 18] = [ 
        (0x30, 192),
        (0x32, 192),
        (0x35, 192),
        (0x37, 192),
        (0x50, 128),
        (0x52, 128),
        (0x53, 128),
        (0x60,  64),
        (0x61, 128),
        (0x62, 128),
        (0x70,  64),
        (0x72,  64),
        (0x75,  64),
        (0x80,  64),
        (0x86, 128),
        (0x87, 128),
        (0x89, 128),
        (0x90, 128),
    ];

    let cores_per_sm = gpu_arch_to_sms.iter()
        .find(|elem| elem.0 == (major << 4) + minor)
        .map(|elem| elem.1);

    match cores_per_sm {
        Some(count) => count,
        None => {
            let default = gpu_arch_to_sms.last().unwrap().1;
            error!(
                "MapSMtoCores for SM {}.{} is undefined.Default to use {} Cores/SM",
                major, minor, default
            );
            default
        }
    }
}
