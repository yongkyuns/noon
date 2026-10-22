//! Semantic-precision conveniences over the ordinary path transaction.
use super::scalar::{ScalarPathEdit as Edit, LINE_SAMPLES};
use super::*;
impl Mobject {
    /// Begin a world-space subpath without lowering its anchor to renderer precision.
    pub fn start_new_path_f64(&mut self, point: (f64, f64)) -> Result<(), AuthoringError> {
        self.start_new_path_f64_with_stride(point, 4)
    }
    /// Begin a subpath using the supplied point-array stride.
    pub fn start_new_path_f64_with_stride(
        &mut self,
        point: (f64, f64),
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::Start { point, stride }))
    }
    /// Append a line using double-precision interpolation and coordinates.
    pub fn add_line_to_f64(&mut self, point: (f64, f64)) -> Result<(), AuthoringError> {
        self.add_line_to_f64_with_stride_and_samples(point, 4, &LINE_SAMPLES)
    }
    /// Append a line using the supplied point-array stride.
    pub fn add_line_to_f64_with_stride(
        &mut self,
        point: (f64, f64),
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.add_line_to_f64_with_stride_and_samples(point, stride, &LINE_SAMPLES)
    }
    /// Append a line using the supplied stride and interpolation samples.
    pub fn add_line_to_f64_with_stride_and_samples(
        &mut self,
        point: (f64, f64),
        stride: i32,
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::Line {
            point,
            stride,
            samples,
        }))
    }
    /// Append cubic handles and an anchor at semantic precision.
    pub fn add_cubic_bezier_curve_to_f64(
        &mut self,
        control1: (f64, f64),
        control2: (f64, f64),
        anchor: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.add_cubic_bezier_curve_to_f64_with_stride(control1, control2, anchor, 4)
    }
    /// Append cubic controls using the supplied point-array stride.
    pub fn add_cubic_bezier_curve_to_f64_with_stride(
        &mut self,
        control1: (f64, f64),
        control2: (f64, f64),
        anchor: (f64, f64),
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::CubicTo {
            controls: [control1, control2, anchor],
            stride,
        }))
    }
    /// Append all four cubic controls without lowering the existing path.
    pub fn add_cubic_bezier_curve_f64(
        &mut self,
        anchor1: (f64, f64),
        handle1: (f64, f64),
        handle2: (f64, f64),
        anchor2: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::Cubic([
            anchor1, handle1, handle2, anchor2,
        ])))
    }
    /// Replace the path with double-precision line controls between corners.
    pub fn set_points_as_corners_f64(
        &mut self,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        self.set_points_as_corners_f64_with_samples(points, &LINE_SAMPLES)
    }
    /// Replace the path using the supplied corner interpolation samples.
    pub fn set_points_as_corners_f64_with_samples(
        &mut self,
        points: &[(f64, f64)],
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::Corners { points, samples }))
    }
    /// Append corners while preserving the existing semantic-precision prefix.
    pub fn add_points_as_corners_f64(
        &mut self,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        self.add_points_as_corners_f64_with_stride(points, 4, &LINE_SAMPLES)
    }
    /// Append corners using the supplied stride and interpolation samples.
    pub fn add_points_as_corners_f64_with_stride(
        &mut self,
        points: &[(f64, f64)],
        stride: i32,
        samples: &[f64],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::AppendCorners {
            points,
            stride,
            samples,
        }))
    }
    /// Append cubic batches without lowering incoming or existing coordinates.
    pub fn add_cubic_bezier_curves_f64(
        &mut self,
        curves: &[[(f64, f64); 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::Cubics {
            curves,
            replace: false,
        }))
    }
    /// Replace the path with the supplied semantic-precision cubic batches.
    pub fn set_cubic_bezier_curves_f64(
        &mut self,
        curves: &[[(f64, f64); 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::Scalar(Edit::Cubics {
            curves,
            replace: true,
        }))
    }
}
impl crate::Scene {
    fn edit_scalar_path(&mut self, object: &Mobject, edit: Edit<'_>) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(store, root, execution, object, PathEdit::Scalar(edit))
        })
    }
    /// Begin a world-space subpath without lowering its anchor to renderer precision.
    pub fn start_new_path_f64(
        &mut self,
        object: &Mobject,
        point: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.start_new_path_f64_with_stride(object, point, 4)
    }
    /// Begin a subpath using the supplied point-array stride.
    pub fn start_new_path_f64_with_stride(
        &mut self,
        object: &Mobject,
        point: (f64, f64),
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(object, Edit::Start { point, stride })
    }
    /// Append a line using double-precision interpolation and coordinates.
    pub fn add_line_to_f64(
        &mut self,
        object: &Mobject,
        point: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.add_line_to_f64_with_stride_and_samples(object, point, 4, &LINE_SAMPLES)
    }
    /// Append a line using the supplied point-array stride.
    pub fn add_line_to_f64_with_stride(
        &mut self,
        object: &Mobject,
        point: (f64, f64),
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.add_line_to_f64_with_stride_and_samples(object, point, stride, &LINE_SAMPLES)
    }
    /// Append a line using the supplied stride and interpolation samples.
    pub fn add_line_to_f64_with_stride_and_samples(
        &mut self,
        object: &Mobject,
        point: (f64, f64),
        stride: i32,
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(
            object,
            Edit::Line {
                point,
                stride,
                samples,
            },
        )
    }
    /// Append cubic handles and an anchor at semantic precision.
    pub fn add_cubic_bezier_curve_to_f64(
        &mut self,
        object: &Mobject,
        control1: (f64, f64),
        control2: (f64, f64),
        anchor: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.add_cubic_bezier_curve_to_f64_with_stride(object, control1, control2, anchor, 4)
    }
    /// Append cubic controls using the supplied point-array stride.
    pub fn add_cubic_bezier_curve_to_f64_with_stride(
        &mut self,
        object: &Mobject,
        control1: (f64, f64),
        control2: (f64, f64),
        anchor: (f64, f64),
        stride: i32,
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(
            object,
            Edit::CubicTo {
                controls: [control1, control2, anchor],
                stride,
            },
        )
    }
    /// Append all four cubic controls without lowering the existing path.
    pub fn add_cubic_bezier_curve_f64(
        &mut self,
        object: &Mobject,
        anchor1: (f64, f64),
        handle1: (f64, f64),
        handle2: (f64, f64),
        anchor2: (f64, f64),
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(object, Edit::Cubic([anchor1, handle1, handle2, anchor2]))
    }
    /// Replace the path with double-precision line controls between corners.
    pub fn set_points_as_corners_f64(
        &mut self,
        object: &Mobject,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        self.set_points_as_corners_f64_with_samples(object, points, &LINE_SAMPLES)
    }
    /// Replace the path using the supplied corner interpolation samples.
    pub fn set_points_as_corners_f64_with_samples(
        &mut self,
        object: &Mobject,
        points: &[(f64, f64)],
        samples: &[f64; 4],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(object, Edit::Corners { points, samples })
    }
    /// Append corners while preserving the existing semantic-precision prefix.
    pub fn add_points_as_corners_f64(
        &mut self,
        object: &Mobject,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        self.add_points_as_corners_f64_with_stride(object, points, 4, &LINE_SAMPLES)
    }
    /// Append corners using the supplied stride and interpolation samples.
    pub fn add_points_as_corners_f64_with_stride(
        &mut self,
        object: &Mobject,
        points: &[(f64, f64)],
        stride: i32,
        samples: &[f64],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(
            object,
            Edit::AppendCorners {
                points,
                stride,
                samples,
            },
        )
    }
    /// Append cubic batches without lowering incoming or existing coordinates.
    pub fn add_cubic_bezier_curves_f64(
        &mut self,
        object: &Mobject,
        curves: &[[(f64, f64); 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(
            object,
            Edit::Cubics {
                curves,
                replace: false,
            },
        )
    }
    /// Replace the path with the supplied semantic-precision cubic batches.
    pub fn set_cubic_bezier_curves_f64(
        &mut self,
        object: &Mobject,
        curves: &[[(f64, f64); 4]],
    ) -> Result<(), AuthoringError> {
        self.edit_scalar_path(
            object,
            Edit::Cubics {
                curves,
                replace: true,
            },
        )
    }
}
