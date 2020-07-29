#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

//use rand::prelude::*;
use rand;
use root::taso::*;
use std::convert::TryInto;
use std::time::{Duration, Instant};

use egg::*;

// Operator parameters, value matches the TASO side
pub const PSAME: i32 = 0;
pub const PVALID: i32 = 1;

pub const ACTNONE: i32 = 0;
pub const ACTSIGMOID: i32 = 1;
pub const ACTRELU: i32 = 2;
pub const ACTTANH: i32 = 3;

define_language! {
    pub enum Mdl {
        "input"     = Input([Id; 1]), // takes a Var, format: name@dim1_dim2...
        "weight"    = Weight([Id; 1]), // takes a Var, format : name@dim1_dim2...
        "ewadd"     = Ewadd([Id; 2]),
        "ewmul"     = Ewmul([Id; 2]),
        "smul"      = Smul([Id; 2]),
        "transpose" = Transpose(Id),
        "matmul"    = Matmul([Id; 3]), // activation, input1, input2
        "conv2d"    = Conv2d([Id; 6]), // conv2d's weight tensor kernel size can not be even, it seems that TASO's output shape computation is incorrect for even kernal size (like 4x4)
        "enlarge"   = Enlarge([Id; 2]), // input_to_enlarge, ref_input
        "relu"      = Relu(Id),
        "tanh"      = Tanh(Id),
        "sigmoid"   = Sigmoid(Id),
        "poolavg"   = Poolavg([Id; 7]), // input, kernel_h, kernel_w, stride_h, stride_w, padding, activation
        "poolmax"   = Poolmax([Id; 7]), // input, kernel_h, kernel_w, stride_h, stride_w, padding, activation
        "concat"    = Concat([Id; 4]), // axis, ndim, input1, input2. ndim is for using in CheckApply only
        "split_0"   = Split0(Id), // must take a split node as input
        "split_1"   = Split1(Id), // must take a split node as input
        "split"     = Split([Id; 2]), // axis, input
        "Cpool"     = Cpool([Id; 2]),
        "Iconv"     = Iconv([Id; 2]),
        "Imatmul"   = Imatmul,
        "Iewmul"    = Iewmul,
        "merge"     = Merge([Id; 2]), // merge_gconv, takes [weight, count]
        Num(i32),
        Var(Symbol),
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DataKind {
    Name,
    Scalar,
    Tnsr,
    TnsrTuple,
}

impl Default for DataKind {
    fn default() -> Self {
        DataKind::Name
    }
}

/// Metadata struct for TensorAnalysis
#[derive(Debug, Clone)]
pub struct ValTnsr {
    /// The data type of this eclass, can be a name/scalar/tensor
    pub dtype: DataKind,
    /// The value of this eclass if it is a Scalar type
    pub val: i32,
    /// The name string of this eclass if it is a Name type
    pub name: String,
    /// The pointer to the tensor if it is a Tensor type
    pub meta: TensorHandle,
    /// The pointer to the second tensor if it is a TnsrTuple type (for split node)
    pub meta_2: TensorHandle,
}

impl Default for ValTnsr {
    fn default() -> Self {
        ValTnsr {
            meta: std::ptr::null_mut(),
            meta_2: std::ptr::null_mut(),
            ..Default::default()
        }
    }
}

/// Struct for metadata analysis
///
/// In this analysis, it calls functions on the TASO side (e.g. graph.matmul())
/// to create (or get) new ops/nodes and stores pointers to the output tensors.
/// TASO will measure and store the runtime cost when creating a new op/node.
pub struct TensorAnalysis {
    /// Points to the graph object on the TASO side
    pub graph: std::cell::RefCell<Box<Graph>>,
}

impl Default for TensorAnalysis {
    fn default() -> Self {
        unsafe {
            // NOTE Box heap-allocates, otherwise any pointer from
            // C++ may be dangling
            let mut graph = Box::new(Graph::new());
            Graph_Graph(&mut *graph);
            TensorAnalysis {
                graph: std::cell::RefCell::new(graph),
            }
        }
    }
}

impl Analysis<Mdl> for TensorAnalysis {
    type Data = ValTnsr;

    /// Merges two metadata when two eclasses are merged. Because the useful
    /// parts of the metadata of two equivalent eclasses are always the same,
    /// we don't need to change
    fn merge(&self, to: &mut Self::Data, from: Self::Data) -> bool {
        false
    }

    // Constructs metadata for a new enode, using TASO side functions for tensors.
    fn make(egraph: &EGraph<Mdl, Self>, enode: &Mdl) -> Self::Data {
        let x = |i: &Id| &egraph[*i].data;
        let dim_from_name = |name: &Id| {
            let name_vec: Vec<&str> = x(name).name.split("@").collect();
            assert!(name_vec.len() == 2);
            let dims: Vec<i32> = name_vec[1]
                .split("_")
                .map(|x| x.parse::<i32>().unwrap())
                .collect();
            dims
        };

        let mut g = egraph.analysis.graph.borrow_mut();
        match enode {
            Mdl::Matmul([act, a, b]) => {
                // Check types
                assert!(x(act).dtype == DataKind::Scalar);
                assert!(x(a).dtype == DataKind::Tnsr);
                assert!(x(b).dtype == DataKind::Tnsr);

                // Get arguments
                let t_a = x(a).meta;
                let t_b = x(b).meta;
                let activation: ActiMode = x(act).val.try_into().unwrap();

                // Create tensorhandle and get metadata
                let res = unsafe { g.matmul(t_a, t_b, activation) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Conv2d([stride_h, stride_w, pad, act, inpt, wght]) => {
                // Check types
                assert!(x(stride_h).dtype == DataKind::Scalar);
                assert!(x(stride_w).dtype == DataKind::Scalar);
                assert!(x(pad).dtype == DataKind::Scalar);
                assert!(x(act).dtype == DataKind::Scalar);
                assert!(x(inpt).dtype == DataKind::Tnsr);
                assert!(x(wght).dtype == DataKind::Tnsr);

                // Get arguments
                let t_inpt = x(inpt).meta;
                let t_wght = x(wght).meta;
                let strideH = x(stride_h).val;
                let strideW = x(stride_w).val;
                let padding: PaddingMode = x(pad).val.try_into().unwrap();
                let activation: ActiMode = x(act).val.try_into().unwrap();

                // Create tensorhandle and get metadata
                let res =
                    unsafe { g.conv2d1(t_inpt, t_wght, strideH, strideW, padding, activation) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Ewadd([a, b]) => {
                // Check types
                assert!(x(a).dtype == DataKind::Tnsr);
                assert!(x(b).dtype == DataKind::Tnsr);

                // Get arguments
                let t_a = x(a).meta;
                let t_b = x(b).meta;

                // Create tensorhandle and get metadata
                let res = unsafe { g.element(OpType_OP_EW_ADD, t_a, t_b) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Ewmul([a, b]) => {
                // Check types
                assert!(x(a).dtype == DataKind::Tnsr);
                assert!(x(b).dtype == DataKind::Tnsr);

                // Get arguments
                let t_a = x(a).meta;
                let t_b = x(b).meta;

                // Create tensorhandle and get metadata
                let res = unsafe { g.element(OpType_OP_EW_MUL, t_a, t_b) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Relu(a) => {
                assert!(x(a).dtype == DataKind::Tnsr);
                let t_a = x(a).meta;

                let res = unsafe { g.relu(t_a, true) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Tanh(a) => {
                assert!(x(a).dtype == DataKind::Tnsr);
                let t_a = x(a).meta;

                let res = unsafe { g.tanh(t_a, true) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Sigmoid(a) => {
                assert!(x(a).dtype == DataKind::Tnsr);
                let t_a = x(a).meta;

                let res = unsafe { g.sigmoid(t_a, true) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Input([name]) => {
                // Check types
                assert!(x(name).dtype == DataKind::Name);

                // Get arguments
                let mut dims = dim_from_name(name);
                let ndim = dims.len();
                dims.shrink_to_fit();
                assert!(dims.len() == dims.capacity());
                let ptr = dims.as_mut_ptr();
                std::mem::forget(dims);

                // Create tensorhandle and get metadata
                let res = unsafe { g.new_input(ndim.try_into().unwrap(), ptr) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Weight([name]) => {
                // Check types
                assert!(x(name).dtype == DataKind::Name);

                // Get arguments
                let mut dims = dim_from_name(name);
                let ndim = dims.len();
                dims.shrink_to_fit();
                assert!(dims.len() == dims.capacity());

                let num_entries = dims.iter().product();
                let mut weight_data: Vec<f32> = (0..num_entries).map(|_| rand::random()).collect();
                weight_data.shrink_to_fit();
                assert!(weight_data.len() == weight_data.capacity());

                let ptr = dims.as_mut_ptr();
                std::mem::forget(dims);
                let data_ptr = weight_data.as_mut_ptr();
                std::mem::forget(weight_data);

                // Create tensorhandle and get metadata
                let res = unsafe { g.new_weight(ndim.try_into().unwrap(), ptr, data_ptr) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Concat([axis, ndim, a, b]) => {
                // Check types
                assert!(x(axis).dtype == DataKind::Scalar);
                assert!(x(ndim).dtype == DataKind::Scalar);
                assert!(x(a).dtype == DataKind::Tnsr);
                assert!(x(b).dtype == DataKind::Tnsr);

                // Get arguments
                let t_a = x(a).meta;
                let t_b = x(b).meta;
                let axis_val = x(axis).val;

                // Create tensorhandle and get metadata
                let t = [t_a, t_b];
                let res = unsafe { g.concat(axis_val, 2, t.as_ptr()) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Merge([weight, count]) => {
                // Check types
                assert!(x(count).dtype == DataKind::Scalar);
                assert!(x(weight).dtype == DataKind::Tnsr);

                // Get arguments
                let t_weight = x(weight).meta;
                let count_val = x(count).val;

                // Create tensorhandle and get metadata
                let res = unsafe { g.merge_gconv(t_weight, count_val) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Poolmax([inpt, kernel_h, kernel_w, stride_h, stride_w, pad, act]) => {
                // Check types
                assert!(x(kernel_h).dtype == DataKind::Scalar);
                assert!(x(kernel_w).dtype == DataKind::Scalar);
                assert!(x(stride_h).dtype == DataKind::Scalar);
                assert!(x(stride_w).dtype == DataKind::Scalar);
                assert!(x(pad).dtype == DataKind::Scalar);
                assert!(x(act).dtype == DataKind::Scalar);
                assert!(x(inpt).dtype == DataKind::Tnsr);

                // Get arguments
                let t_inpt = x(inpt).meta;
                let kernelH = x(kernel_h).val;
                let kernelW = x(kernel_w).val;
                let strideH = x(stride_h).val;
                let strideW = x(stride_w).val;
                let padding: PaddingMode = x(pad).val.try_into().unwrap();
                let activation: ActiMode = x(act).val.try_into().unwrap();

                // Create tensorhandle and get metadata
                let res = unsafe {
                    g.pool2d_max(
                        t_inpt, kernelH, kernelW, strideH, strideW, padding, activation,
                    )
                };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Split([axis, inpt]) => {
                // Check types
                assert!(x(axis).dtype == DataKind::Scalar);
                assert!(x(inpt).dtype == DataKind::Tnsr);

                // Get arguments
                let t_inpt = x(inpt).meta;
                let axis_val = x(axis).val;

                // Create tensorhandle and get metadata
                unsafe {
                    let op = (*g.model).get_or_create_split1(t_inpt, axis_val, 2);
                    assert!(op != Op_INVALID_OP);
                    g.add_edge((*t_inpt).op, op, (*t_inpt).idx, 0);
                    let x1 = Box::new((*op.ptr).outputs[0].clone());
                    let res = Box::into_raw(x1);
                    let x2 = Box::new((*op.ptr).outputs[1].clone());
                    let res_2 = Box::into_raw(x2);
                    Self::Data {
                        dtype: DataKind::TnsrTuple,
                        val: 0,
                        name: String::new(),
                        meta: res,
                        meta_2: res_2,
                    }
                }
            }

            Mdl::Split0(inpt) => {
                // Check types
                assert!(x(inpt).dtype == DataKind::TnsrTuple);

                let res = x(inpt).meta;
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Split1(inpt) => {
                // Check types
                assert!(x(inpt).dtype == DataKind::TnsrTuple);

                let res = x(inpt).meta_2;
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Enlarge([a, b]) => {
                // Check types
                assert!(x(a).dtype == DataKind::Tnsr);
                assert!(x(b).dtype == DataKind::Tnsr);

                // Get arguments
                let t_a = x(a).meta;
                let t_b = x(b).meta;

                // Create tensorhandle and get metadata
                let res = unsafe { g.enlarge(t_a, t_b) };
                Self::Data {
                    dtype: DataKind::Tnsr,
                    val: 0,
                    name: String::new(),
                    meta: res,
                    meta_2: std::ptr::null_mut(),
                }
            }

            Mdl::Num(_n) => Self::Data {
                dtype: DataKind::Scalar,
                val: *_n,
                name: String::new(),
                meta: std::ptr::null_mut(),
                meta_2: std::ptr::null_mut(),
            },

            Mdl::Var(_s) => Self::Data {
                dtype: DataKind::Name,
                val: 0,
                name: _s.as_str().to_string(),
                meta: std::ptr::null_mut(),
                meta_2: std::ptr::null_mut(),
            },

            other => {
                println!("{:?}", other);
                todo!()
            }
        }
    }

    // Not needed to modify anything
    fn modify(egraph: &mut EGraph<Mdl, Self>, id: Id) {}
}
