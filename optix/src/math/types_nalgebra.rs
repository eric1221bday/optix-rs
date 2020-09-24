use crate::impl_device_copy_align;
use crate::DeviceCopy;
use crate::Vertex;
use crate::IndexTriple;
use crate::VertexFormat;
use crate::IndicesFormat;

impl_device_copy_align!(
    nalgebra::Point3<f32>:4
    nalgebra::Vector3<u32>:4
);

impl Vertex for nalgebra::Point3<f32> {
    const FORMAT: VertexFormat = VertexFormat::Float3;
}

impl IndexTriple for nalgebra::Vector3<u32> {
    const FORMAT: IndicesFormat = IndicesFormat::Int3;
}
