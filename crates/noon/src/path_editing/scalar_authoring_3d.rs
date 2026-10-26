//! Full-coordinate conveniences over the ordinary path transaction.
use super::scalar::{ScalarPathEdit as Edit, LINE_SAMPLES};
use super::*;
impl Mobject {
    /// Begin a world-space subpath without lowering its anchor to renderer precision.
    pub fn start_new_path_3d(&mut self, point: SemanticVec3) -> Result<(), AuthoringError> {
        self.start_new_path_3d_with_stride(point, 4)
    }
    /// Begin a subpath using the supplied point-array stride.
    pub fn start_new_path_3d_with_stride(
        &mut self,
        point: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::Start { point, stride }))
    }
    /// Append a line using double-precision interpolation and coordinates.
    pub fn add_line_to_3d(&mut self, point: SemanticVec3) -> Result<(), AuthoringError> {
        self.add_line_to_3d_with_stride_and_samples(point, 4, &LINE_SAMPLES)
    }
    /// Append a line using the supplied point-array stride.
    pub fn add_line_to_3d_with_stride(
        &mut self,
        point: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.add_line_to_3d_with_stride_and_samples(point, stride, &LINE_SAMPLES)
    }
    /// Append a line using the supplied stride and interpolation samples.
    pub fn add_line_to_3d_with_stride_and_samples(
        &mut self,
        point: SemanticVec3,
        stride: i32,
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::Line {
            point,
            stride,
            samples,
        }))
    }
    /// Append cubic handles and an anchor at semantic precision.
    pub fn add_cubic_bezier_curve_to_3d(
        &mut self,
        control1: SemanticVec3,
        control2: SemanticVec3,
        anchor: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.add_cubic_bezier_curve_to_3d_with_stride(control1, control2, anchor, 4)
    }
    /// Append cubic controls using the supplied point-array stride.
    pub fn add_cubic_bezier_curve_to_3d_with_stride(
        &mut self,
        control1: SemanticVec3,
        control2: SemanticVec3,
        anchor: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::CubicTo {
            controls: [control1, control2, anchor],
            stride,
        }))
    }
    /// Append all four cubic controls without lowering the existing path.
    pub fn add_cubic_bezier_curve_3d(
        &mut self,
        anchor1: SemanticVec3,
        handle1: SemanticVec3,
        handle2: SemanticVec3,
        anchor2: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::Cubic([
            anchor1, handle1, handle2, anchor2,
        ])))
    }
    /// Replace the path with double-precision line controls between corners.
    pub fn set_points_as_corners_3d(
        &mut self,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.set_points_as_corners_3d_with_samples(points, &LINE_SAMPLES)
    }
    /// Replace the path using the supplied corner interpolation samples.
    pub fn set_points_as_corners_3d_with_samples(
        &mut self,
        points: &[SemanticVec3],
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::Corners { points, samples }))
    }
    /// Append corners while preserving the existing semantic-precision prefix.
    pub fn add_points_as_corners_3d(
        &mut self,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.add_points_as_corners_3d_with_stride(points, 4, &LINE_SAMPLES)
    }
    /// Append corners using the supplied stride and interpolation samples.
    pub fn add_points_as_corners_3d_with_stride(
        &mut self,
        points: &[SemanticVec3],
        stride: i32,
        samples: &[f64],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::AppendCorners {
            points,
            stride,
            samples,
        }))
    }
    /// Append cubic batches without lowering incoming or existing coordinates.
    pub fn add_cubic_bezier_curves_3d(
        &mut self,
        curves: &[[SemanticVec3; 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::Cubics {
            curves,
            replace: false,
        }))
    }
    /// Replace the path with the supplied semantic-precision cubic batches.
    pub fn set_cubic_bezier_curves_3d(
        &mut self,
        curves: &[[SemanticVec3; 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar3d(Edit::Cubics {
            curves,
            replace: true,
        }))
    }
}
impl crate::Scene {
    fn edit_scalar_path_3d(
        &mut self,
        object: &Mobject,
        edit: Edit<'_, SemanticVec3>,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Scalar3d(edit))
        })
    }
    /// Begin a world-space subpath without lowering its anchor to renderer precision.
    pub fn start_new_path_3d(
        &mut self,
        object: &Mobject,
        point: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.start_new_path_3d_with_stride(object, point, 4)
    }
    /// Begin a subpath using the supplied point-array stride.
    pub fn start_new_path_3d_with_stride(
        &mut self,
        object: &Mobject,
        point: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(object, Edit::Start { point, stride })
    }
    /// Append a line using double-precision interpolation and coordinates.
    pub fn add_line_to_3d(
        &mut self,
        object: &Mobject,
        point: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.add_line_to_3d_with_stride_and_samples(object, point, 4, &LINE_SAMPLES)
    }
    /// Append a line using the supplied point-array stride.
    pub fn add_line_to_3d_with_stride(
        &mut self,
        object: &Mobject,
        point: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.add_line_to_3d_with_stride_and_samples(object, point, stride, &LINE_SAMPLES)
    }
    /// Append a line using the supplied stride and interpolation samples.
    pub fn add_line_to_3d_with_stride_and_samples(
        &mut self,
        object: &Mobject,
        point: SemanticVec3,
        stride: i32,
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(
            object,
            Edit::Line {
                point,
                stride,
                samples,
            },
        )
    }
    /// Append cubic handles and an anchor at semantic precision.
    pub fn add_cubic_bezier_curve_to_3d(
        &mut self,
        object: &Mobject,
        control1: SemanticVec3,
        control2: SemanticVec3,
        anchor: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.add_cubic_bezier_curve_to_3d_with_stride(object, control1, control2, anchor, 4)
    }
    /// Append cubic controls using the supplied point-array stride.
    pub fn add_cubic_bezier_curve_to_3d_with_stride(
        &mut self,
        object: &Mobject,
        control1: SemanticVec3,
        control2: SemanticVec3,
        anchor: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(
            object,
            Edit::CubicTo {
                controls: [control1, control2, anchor],
                stride,
            },
        )
    }
    /// Append all four cubic controls without lowering the existing path.
    pub fn add_cubic_bezier_curve_3d(
        &mut self,
        object: &Mobject,
        anchor1: SemanticVec3,
        handle1: SemanticVec3,
        handle2: SemanticVec3,
        anchor2: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(object, Edit::Cubic([anchor1, handle1, handle2, anchor2]))
    }
    /// Replace the path with double-precision line controls between corners.
    pub fn set_points_as_corners_3d(
        &mut self,
        object: &Mobject,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.set_points_as_corners_3d_with_samples(object, points, &LINE_SAMPLES)
    }
    /// Replace the path using the supplied corner interpolation samples.
    pub fn set_points_as_corners_3d_with_samples(
        &mut self,
        object: &Mobject,
        points: &[SemanticVec3],
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(object, Edit::Corners { points, samples })
    }
    /// Append corners while preserving the existing semantic-precision prefix.
    pub fn add_points_as_corners_3d(
        &mut self,
        object: &Mobject,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.add_points_as_corners_3d_with_stride(object, points, 4, &LINE_SAMPLES)
    }
    /// Append corners using the supplied stride and interpolation samples.
    pub fn add_points_as_corners_3d_with_stride(
        &mut self,
        object: &Mobject,
        points: &[SemanticVec3],
        stride: i32,
        samples: &[f64],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(
            object,
            Edit::AppendCorners {
                points,
                stride,
                samples,
            },
        )
    }
    /// Append cubic batches without lowering incoming or existing coordinates.
    pub fn add_cubic_bezier_curves_3d(
        &mut self,
        object: &Mobject,
        curves: &[[SemanticVec3; 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(
            object,
            Edit::Cubics {
                curves,
                replace: false,
            },
        )
    }
    /// Replace the path with the supplied semantic-precision cubic batches.
    pub fn set_cubic_bezier_curves_3d(
        &mut self,
        object: &Mobject,
        curves: &[[SemanticVec3; 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path_3d(
            object,
            Edit::Cubics {
                curves,
                replace: true,
            },
        )
    }
}

impl Mobject {
    pub fn add_quadratic_bezier_curve_to_3d(
        &mut self,
        control: SemanticVec3,
        anchor: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.add_quadratic_bezier_curve_to_3d_with_stride(control, anchor, 4)
    }
    pub fn add_quadratic_bezier_curve_to_3d_with_stride(
        &mut self,
        control: SemanticVec3,
        anchor: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Quadratic3dWithStride(control, anchor, stride))
    }
}
impl crate::Scene {
    pub fn add_quadratic_bezier_curve_to_3d(
        &mut self,
        object: &Mobject,
        control: SemanticVec3,
        anchor: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.add_quadratic_bezier_curve_to_3d_with_stride(object, control, anchor, 4)
    }
    pub fn add_quadratic_bezier_curve_to_3d_with_stride(
        &mut self,
        object: &Mobject,
        control: SemanticVec3,
        anchor: SemanticVec3,
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Quadratic3dWithStride(control, anchor, stride),
            )
        })
    }
}
