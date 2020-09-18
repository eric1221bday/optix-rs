use cu::{
    allocator::{DeviceFrameAllocator, Layout},
    DevicePtr,
};

pub use optix::{DeviceContext, DeviceStorage, Error};

use crate::{V2f32, V3f32, V3i32, V4f32};

pub use anyhow::{Context, Result};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use ustr::ustr;

pub use crate::vector::*;

// Global device allocator
// The DeviceFrameAllocator here is a very dumb bump allocator. You'll want to
// write something a bit cleverer for a real program (or just use the default
// allocator if you don't care).
// Anything that allocates has an `_in()` variant that allows you to specify
// the allocator you want to get memory from.
static FRAME_ALLOC: Lazy<Mutex<DeviceFrameAllocator>> = Lazy::new(|| {
    Mutex::new(
        // We'll use a block size of 256MB. When a block is exhausted, it will
        // just get another from the default allocator
        DeviceFrameAllocator::new(256 * 1024 * 1024)
            .expect("Frame allocator failed"),
    )
});

// We set up a unit struct as a handle to the global alloc so we have an object
// we can pass to allocating constructors
pub struct FrameAlloc;
unsafe impl cu::allocator::DeviceAllocRef for FrameAlloc {
    fn alloc(&self, layout: Layout) -> Result<DevicePtr, cu::Error> {
        FRAME_ALLOC.lock().alloc(layout)
    }

    fn alloc_with_tag(
        &self,
        layout: Layout,
        tag: u16,
    ) -> Result<DevicePtr, cu::Error> {
        FRAME_ALLOC.lock().alloc_with_tag(layout, tag)
    }

    fn alloc_pitch(
        &self,
        width_in_bytes: usize,
        height_in_rows: usize,
        element_byte_size: usize,
    ) -> Result<(DevicePtr, usize), cu::Error> {
        FRAME_ALLOC.lock().alloc_pitch(
            width_in_bytes,
            height_in_rows,
            element_byte_size,
        )
    }

    fn alloc_pitch_with_tag(
        &self,
        width_in_bytes: usize,
        height_in_rows: usize,
        element_byte_size: usize,
        tag: u16,
    ) -> Result<(DevicePtr, usize), cu::Error> {
        FRAME_ALLOC.lock().alloc_pitch_with_tag(
            width_in_bytes,
            height_in_rows,
            element_byte_size,
            tag,
        )
    }

    fn dealloc(&self, ptr: DevicePtr) -> Result<(), cu::Error> {
        FRAME_ALLOC.lock().dealloc(ptr)
    }
}

pub fn alloc_mem_report() {
    let (num_allocs, total_allocated) = FRAME_ALLOC.lock().report();
    println!(
        "{} bytes allocated in {} allocations",
        total_allocated, num_allocs
    );
}

pub struct Renderer {
    stream: cu::Stream,
    launch_params: optix::DeviceVariable<LaunchParams, FrameAlloc>,
    buf_vertex: Vec<optix::TypedBuffer<V3f32, FrameAlloc>>,
    buf_index: Vec<optix::TypedBuffer<V3i32, FrameAlloc>>,
    buf_normal: Vec<Option<optix::TypedBuffer<V3f32, FrameAlloc>>>,
    buf_texcoord: Vec<Option<optix::TypedBuffer<V2f32, FrameAlloc>>>,
    buf_raygen: optix::TypedBuffer<RaygenRecord, FrameAlloc>,
    buf_hitgroup: optix::TypedBuffer<HitgroupRecord, FrameAlloc>,
    buf_miss: optix::TypedBuffer<MissRecord, FrameAlloc>,
    as_handle: optix::TraversableHandle,
    as_buffer: optix::Buffer<FrameAlloc>,
    sbt: optix::sys::OptixShaderBindingTable,
    pipeline: optix::Pipeline,
    color_buffer: optix::TypedBuffer<V4f32, FrameAlloc>,
    accumulate: bool,
    denoiser: optix::Denoiser,
}

impl Renderer {
    pub fn new(
        width: u32,
        height: u32,
        camera: Camera,
        model: Model,
        light: Light,
    ) -> Result<Renderer> {
        // Initialize CUDA and check we've got a suitable device
        cu::init()?;
        let device_count = cu::Device::get_count()?;
        if device_count == 0 {
            panic!("No CUDA devices found!");
        }

        // Initialize optix. This just loads the functions from the driver and
        // must be called before any other optix function
        optix::init()?;

        // Create CUDA and OptiX contexts
        let device = cu::Device::get(0)?;

        let cuda_context = device.ctx_create(
            cu::ContextFlags::SCHED_AUTO | cu::ContextFlags::MAP_HOST,
        )?;
        let stream = cu::Stream::create(cu::StreamFlags::DEFAULT)?;

        let mut ctx = optix::DeviceContext::create(&cuda_context)?;
        // Set up logging callback with a closure
        ctx.set_log_callback(
            |_level, tag, msg| println!("[{}]: {}", tag, msg),
            4,
        );

        // create module
        let module_compile_options = optix::ModuleCompileOptions {
            max_register_count: 50,
            opt_level: optix::CompileOptimizationLevel::Level0,
            debug_level: optix::CompileDebugLevel::Full,
        };

        // set our compile options
        let pipeline_compile_options = optix::PipelineCompileOptions::new()
            .uses_motion_blur(false)
            .num_attribute_values(2)
            .num_payload_values(2)
            .traversable_graph_flags(
                optix::TraversableGraphFlags::ALLOW_SINGLE_GAS,
            )
            .exception_flags(optix::ExceptionFlags::NONE)
            .pipeline_launch_params_variable_name(ustr("optixLaunchParams"));

        // load our precompiled PTX as a str
        // see build.rs for how this is compiled from the cuda source
        let ptx = include_str!(concat!(
            env!("OUT_DIR"),
            "/examples/11_denoise/device_programs.ptx"
        ));

        let (module, _log) = ctx.module_create_from_ptx(
            &module_compile_options,
            &pipeline_compile_options,
            ptx,
        )?;

        // create raygen program
        let pg_raygen =
            vec![ctx.program_group_raygen(
                &module,
                ustr("__raygen__renderFrame"),
            )?];

        // create miss programs
        let pg_miss = vec![
            ctx.program_group_miss(&module, ustr("__miss__radiance"))?,
            ctx.program_group_miss(&module, ustr("__miss__shadow"))?,
        ];

        // create hitgroup programs
        let pg_hitgroup = vec![
            ctx.program_group_hitgroup(
                Some((&module, ustr("__closesthit__radiance"))),
                Some((&module, ustr("__anyhit__radiance"))),
                None,
            )?,
            ctx.program_group_hitgroup(
                Some((&module, ustr("__closesthit__shadow"))),
                Some((&module, ustr("__anyhit__shadow"))),
                None,
            )?,
        ];

        let mut buf_vertex = Vec::with_capacity(model.meshes.len());
        let mut buf_index = Vec::with_capacity(model.meshes.len());
        let mut buf_normal = Vec::with_capacity(model.meshes.len());
        let mut buf_texcoord = Vec::with_capacity(model.meshes.len());
        let geometry_flags = optix::GeometryFlags::None;

        for mesh in &model.meshes {
            buf_vertex.push(optix::TypedBuffer::from_slice_in(
                &mesh.vertex,
                FrameAlloc,
            )?);

            buf_index.push(optix::TypedBuffer::from_slice_in(
                &mesh.index,
                FrameAlloc,
            )?);

            if !mesh.normal.is_empty() {
                buf_normal.push(Some(optix::TypedBuffer::from_slice_in(
                    &mesh.normal,
                    FrameAlloc,
                )?));
            } else {
                buf_normal.push(None);
            }

            if !mesh.texcoord.is_empty() {
                buf_texcoord.push(Some(optix::TypedBuffer::from_slice_in(
                    &mesh.texcoord,
                    FrameAlloc,
                )?));
            } else {
                buf_texcoord.push(None);
            }
        }

        let triangle_inputs: Vec<_> = buf_vertex
            .iter()
            .zip(&buf_index)
            .map(
                |(vertex, index): (
                    &optix::TypedBuffer<V3f32, FrameAlloc>,
                    &optix::TypedBuffer<V3i32, FrameAlloc>,
                )| {
                    optix::BuildInput::TriangleArray(
                        optix::TriangleArray::new(
                            std::slice::from_ref(vertex),
                            std::slice::from_ref(&geometry_flags),
                        )
                        .index_buffer(index),
                    )
                },
            )
            .collect();

        // upload textures
        let mut buf_texture = Vec::new();
        let mut tex_objects = Vec::new();
        for texture in model.textures {
            let element_byte_size = 4;
            let width_in_bytes =
                texture.resolution.x as usize * element_byte_size;
            let height = texture.resolution.y as usize;
            let (ptr, pitch_in_bytes) = FRAME_ALLOC.lock().alloc_pitch(
                width_in_bytes,
                height,
                element_byte_size,
            )?;
            let tex_object = cu::TexObject::create_pitch2d(
                ptr,
                cu::ArrayFormat::Uint8,
                4,
                texture.resolution.x as usize,
                texture.resolution.y as usize,
                pitch_in_bytes,
            )
            .filter_mode(cu::FilterMode::Linear)
            .address_mode(cu::AddressMode::Wrap)
            .flags(
                cu::TextureReadFlags::NORMALIZED_COORDINATES
                    | cu::TextureReadFlags::SRGB,
            )
            .build()
            .context("texture create")?;

            unsafe {
                cu::memory::memcpy2d_htod(
                    ptr,
                    width_in_bytes,
                    height,
                    pitch_in_bytes,
                    texture.pixels.as_ptr() as *const _,
                    width_in_bytes,
                    height,
                    width_in_bytes,
                )
                .context("texture memcpy")?;
            }

            buf_texture.push(ptr);
            tex_objects.push(tex_object);
        }

        // blas setup
        let accel_options = optix::AccelBuildOptions::new(
            optix::BuildFlags::ALLOW_COMPACTION,
            optix::BuildOperation::Build,
        );

        let blas_buffer_sizes = ctx
            .accel_compute_memory_usage(&[accel_options], &triangle_inputs)
            .context("Accel compute memory usage")?;

        // prepare compaction
        // we need scratch space for the BVH build which we allocate here as
        // an untyped buffer. Note that we need to specify the alignment
        let temp_buffer = optix::Buffer::uninitialized_with_align_in(
            blas_buffer_sizes.temp_size_in_bytes,
            optix::ACCEL_BUFFER_BYTE_ALIGNMENT,
            FrameAlloc,
        )?;

        let output_buffer = optix::Buffer::uninitialized_with_align_in(
            blas_buffer_sizes.output_size_in_bytes,
            optix::ACCEL_BUFFER_BYTE_ALIGNMENT,
            FrameAlloc,
        )?;

        // DeviceVariable is a type that wraps a POD type to allow easy access
        // to the data rather than having to carry around the host type and a
        // separate device allocation for it
        let mut compacted_size =
            optix::DeviceVariable::new_in(0usize, FrameAlloc)?;

        // tell the accel build we want to know the size the compacted buffer
        // will be
        let mut properties = vec![optix::AccelEmitDesc::CompactedSize(
            compacted_size.device_ptr(),
        )];

        // build the bvh
        let as_handle = ctx
            .accel_build(
                &stream,
                &[accel_options],
                &triangle_inputs,
                &temp_buffer,
                &output_buffer,
                &mut properties,
            )
            .context("accel build")?;

        cu::Context::synchronize().context("Accel build sync")?;

        // copy the size back from the device, we can now treat it as
        // the underlying type by `Deref`
        compacted_size.download()?;

        // allocate the final acceleration structure storage
        let as_buffer = optix::Buffer::uninitialized_with_align_in(
            *compacted_size,
            optix::ACCEL_BUFFER_BYTE_ALIGNMENT,
            FrameAlloc,
        )?;

        // compact the accel.
        // we don't need the original handle any more
        let as_handle = ctx
            .accel_compact(&stream, as_handle, &as_buffer)
            .context("Accel compact")?;
        cu::Context::synchronize().context("Accel compact sync")?;

        // create pipeline
        let mut program_groups = Vec::new();
        program_groups.extend(pg_raygen.iter().cloned());
        program_groups.extend(pg_miss.iter().cloned());
        program_groups.extend(pg_hitgroup.iter().cloned());

        let pipeline_link_options = optix::PipelineLinkOptions {
            max_trace_depth: 2,
            debug_level: optix::CompileDebugLevel::Full,
        };

        let (pipeline, _log) = ctx.pipeline_create(
            &pipeline_compile_options,
            pipeline_link_options,
            &program_groups,
        )?;

        pipeline.set_stack_size(2 * 1024, 2 * 1024, 2 * 1024, 1)?;

        // create SBT
        let rec_raygen: Vec<_> = pg_raygen
            .iter()
            .map(|pg| {
                RaygenRecord::pack(0, pg).expect("failed to pack raygen record")
            })
            .collect();

        let rec_miss: Vec<_> = pg_miss
            .iter()
            .map(|pg| {
                MissRecord::pack(0, pg).expect("failed to pack miss record")
            })
            .collect();

        let mut rec_hitgroup = Vec::with_capacity(model.meshes.len() * 2);
        for (i, mesh) in model.meshes.iter().enumerate() {
            for raytype in 0..2 {
                let (has_texture, texture) =
                    if let Some(texid) = mesh.diffuse_texture_id {
                        (true, tex_objects[texid].inner())
                    } else {
                        (false, 0)
                    };

                let rec = HitgroupRecord::pack(
                    HitgroupSbtData {
                        data: TriangleMeshSbtData {
                            color: mesh.diffuse,
                            vertex: buf_vertex[i].device_ptr(),
                            index: buf_index[i].device_ptr(),
                            normal: if let Some(b) = &buf_normal[i] {
                                b.device_ptr()
                            } else {
                                cu::DevicePtr::null()
                            },
                            texcoord: if let Some(b) = &buf_texcoord[i] {
                                b.device_ptr()
                            } else {
                                cu::DevicePtr::null()
                            },
                            has_texture,
                            texture,
                        },
                    },
                    &pg_hitgroup[raytype],
                )?;

                rec_hitgroup.push(rec);
            }
        }

        // Create storage to hold all our SBT records
        // TypedBuffer will take care of the allocation alignment for us as the
        // host type is correctly aligned
        let buf_raygen =
            optix::TypedBuffer::from_slice_in(&rec_raygen, FrameAlloc)?;
        let buf_miss =
            optix::TypedBuffer::from_slice_in(&rec_miss, FrameAlloc)?;
        let buf_hitgroup =
            optix::TypedBuffer::from_slice_in(&rec_hitgroup, FrameAlloc)?;

        let sbt = optix::ShaderBindingTable::new(&buf_raygen)
            .miss(&buf_miss)
            .hitgroup(&buf_hitgroup)
            .build();

        // Allocate storage for our output framebuffer.
        // Note that the element type here is V4f32, which has a natural
        // alignment on the host of 4 bytes. On the device we require 16-byte
        // alignment. This is handled for us because V4f32 implements the
        // DeviceCopy trait and overrides the device_align() method to return
        // 16 bytes, which TypedBuffer uses to allocate the storage correctly
        let color_buffer = optix::TypedBuffer::uninitialized_in(
            (width * height) as usize,
            FrameAlloc,
        )?;

        // camera setup
        let cosfovy = 0.66f32;
        let aspect = width as f32 / height as f32;
        let direction = normalize(camera.at - camera.from);
        let horizontal =
            cosfovy * aspect * normalize(cross(direction, camera.up));
        let vertical = cosfovy * normalize(cross(horizontal, direction));

        // we use DeviceVariable again for easy access to the launch params
        let launch_params = optix::DeviceVariable::new_in(
            LaunchParams {
                num_pixel_samples: 1,
                frame: Frame {
                    frame_id: 0,
                    color_buffer: color_buffer.device_ptr(),
                    size: v2i32(width as i32, height as i32),
                },
                camera: RenderCamera {
                    position: camera.from,
                    direction,
                    horizontal,
                    vertical,
                },
                light,
                traversable: as_handle,
            },
            FrameAlloc,
        )?;

        let denoiser = ctx.denoiser_create(optix::DenoiserInputKind::Rgb)?;

        // Finally, we pack all the buffers that need to persist into the
        // Renderer. All the temporary storage that we don't need to keep around
        // will be cleaned up by RAII on the Buffer types
        Ok(Renderer {
            stream,
            launch_params,
            buf_vertex,
            buf_index,
            buf_normal,
            buf_texcoord,
            buf_raygen,
            buf_hitgroup,
            buf_miss,
            as_handle,
            as_buffer,
            sbt,
            pipeline,
            color_buffer,
            accumulate: true,
            denoiser,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        // resize the output buffer (which will change its address) and put the
        // new buffer into the launch params
        self.color_buffer.resize((width * height) as usize)?;
        self.launch_params.frame.color_buffer = self.color_buffer.device_ptr();
        self.launch_params.frame.size.x = width as i32;
        self.launch_params.frame.size.y = height as i32;
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        // upload the launch params
        if !self.accumulate { self.launch_params.frame.frame_id = 0; }
        self.launch_params.upload()?;
        self.launch_params.frame.frame_id += 1;

        // launch the kernel
        optix::launch(
            &self.pipeline,
            &self.stream,
            &self.launch_params,
            &self.sbt,
            self.launch_params.frame.size.x as u32,
            self.launch_params.frame.size.y as u32,
            1,
        )?;

        // stop-the-world
        cu::Context::synchronize()?;

        Ok(())
    }

    pub fn download_pixels(&self, slice: &mut [V4f32]) -> Result<()> {
        // copy the output color data from the device
        self.color_buffer.download(slice)?;
        Ok(())
    }
}

unsafe impl optix::DeviceCopy for LaunchParams {}

type RaygenRecord = optix::SbtRecord<i32>;
type MissRecord = optix::SbtRecord<i32>;
struct HitgroupSbtData {
    data: TriangleMeshSbtData,
}
unsafe impl optix::DeviceCopy for HitgroupSbtData {}
type HitgroupRecord = optix::SbtRecord<HitgroupSbtData>;

#[repr(C)]
pub struct Frame {
    frame_id: i32,
    color_buffer: cu::DevicePtr,
    size: V2i32,
}

#[repr(C)]
pub struct RenderCamera {
    position: V3f32,
    direction: V3f32,
    horizontal: V3f32,
    vertical: V3f32,
}

#[repr(C)]
pub struct LaunchParams {
    pub num_pixel_samples: i32,
    pub frame: Frame,
    pub camera: RenderCamera,
    pub light: Light,
    pub traversable: optix::TraversableHandle,
}

#[repr(C)]
pub struct Light {
    pub origin: V3f32,
    pub du: V3f32,
    pub dv: V3f32,
    pub power: V3f32,
}

pub struct Camera {
    pub from: V3f32,
    pub at: V3f32,
    pub up: V3f32,
}

#[repr(C)]
struct TriangleMeshSbtData {
    color: V3f32,
    vertex: cu::DevicePtr,
    normal: cu::DevicePtr,
    texcoord: cu::DevicePtr,
    index: cu::DevicePtr,
    has_texture: bool,
    texture: u64,
}

pub struct TriangleMesh {
    pub vertex: Vec<V3f32>,
    pub index: Vec<V3i32>,
    pub color: V3f32,
}

impl TriangleMesh {
    pub fn new(color: V3f32) -> TriangleMesh {
        TriangleMesh {
            vertex: Vec::new(),
            index: Vec::new(),
            color,
        }
    }

    #[cfg(feature = "cgmath")]
    pub fn add_cube(&mut self, center: V3f32, size: V3f32) {
        let start_index = self.vertex.len() as i32;

        use cgmath::ElementWise;

        self.vertex.push(
            v3f32(0.0, 0.0, 0.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 0.0, 0.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(0.0, 1.0, 0.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 1.0, 0.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(0.0, 0.0, 1.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 0.0, 1.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(0.0, 1.0, 1.0).mul_element_wise(size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 1.0, 1.0).mul_element_wise(size) + center - 0.5 * size,
        );

        const indices: [i32; 36] = [
            0, 1, 3, 2, 3, 0, 5, 7, 6, 5, 6, 4, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2,
            7, 6, 1, 5, 6, 1, 7, 3, 4, 0, 2, 4, 2, 6,
        ];

        for c in indices.chunks(3) {
            self.index.push(v3i32(
                c[0] + start_index,
                c[1] + start_index,
                c[2] + start_index,
            ));
        }
    }

    #[cfg(feature = "nalgebra-glm")]
    pub fn add_cube(&mut self, center: V3f32, size: V3f32) {
        let start_index = self.vertex.len() as i32;

        self.vertex.push(
            v3f32(0.0, 0.0, 0.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 0.0, 0.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(0.0, 1.0, 0.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 1.0, 0.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(0.0, 0.0, 1.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 0.0, 1.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(0.0, 1.0, 1.0).component_mul(&size) + center - 0.5 * size,
        );
        self.vertex.push(
            v3f32(1.0, 1.0, 1.0).component_mul(&size) + center - 0.5 * size,
        );

        const indices: [i32; 36] = [
            0, 1, 3, 2, 3, 0, 5, 7, 6, 5, 6, 4, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2,
            7, 6, 1, 5, 6, 1, 7, 3, 4, 0, 2, 4, 2, 6,
        ];

        for c in indices.chunks(3) {
            self.index.push(v3i32(
                c[0] + start_index,
                c[1] + start_index,
                c[2] + start_index,
            ));
        }
    }

    #[cfg(not(any(feature = "cgmath", feature = "nalgebra-glm")))]
    pub fn add_cube(&mut self, center: V3f32, size: V3f32) {
        let start_index = self.vertex.len() as i32;

        self.vertex
            .push((v3f32(0.0, 0.0, 0.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(1.0, 0.0, 0.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(0.0, 1.0, 0.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(1.0, 1.0, 0.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(0.0, 0.0, 1.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(1.0, 0.0, 1.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(0.0, 1.0, 1.0)) * size + center - 0.5 * size);
        self.vertex
            .push((v3f32(1.0, 1.0, 1.0)) * size + center - 0.5 * size);

        const indices: [i32; 36] = [
            0, 1, 3, 2, 3, 0, 5, 7, 6, 5, 6, 4, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2,
            7, 6, 1, 5, 6, 1, 7, 3, 4, 0, 2, 4, 2, 6,
        ];

        for c in indices.chunks(3) {
            self.index.push(v3i32(
                c[0] + start_index,
                c[1] + start_index,
                c[2] + start_index,
            ));
        }
    }
}

pub struct Mesh {
    pub vertex: Vec<V3f32>,
    pub normal: Vec<V3f32>,
    pub texcoord: Vec<V2f32>,
    pub index: Vec<V3i32>,
    pub diffuse: V3f32,
    pub diffuse_texture_id: Option<usize>,
}

pub struct Model {
    pub meshes: Vec<Mesh>,
    pub textures: Vec<Texture>,
    pub bounds: Box3f32,
}

pub struct Texture {
    pub pixels: Vec<u8>,
    pub resolution: V2i32,
}