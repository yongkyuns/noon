//! Angle defaults, quadrants, reflex arcs, elbows and parallel-line emptiness.
use crate::{LiveProgram, ManimAngleOptions, ManimGeometryOptions, Scene, YELLOW};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    for index in 0..6 {
        let x = (index % 3) as f64 * 4.0 - 4.0;
        let y = if index < 3 { 1.7 } else { -1.7 };
        let end = if index == 4 { 0.3 } else { 1.2 };
        let first = [(-1.2, 0.0), (end, 0.0)];
        let second = if index == 5 {
            [(-1.2, 0.5), (1.2, 0.5)]
        } else {
            [(0.0, -1.2), (0.0, 1.2)]
        };
        let mut lines = Vec::new();
        for points in [first, second] {
            let mut request =
                ManimGeometryOptions::line(points[0].0, points[0].1, points[1].0, points[1].1)?;
            request.set_color(
                YELLOW.red.into(),
                YELLOW.green.into(),
                YELLOW.blue.into(),
                1.0,
            )?;
            let mut line = scene.geometry(request)?;
            line.shift(x, y)?;
            lines.push(line);
        }
        let a = lines[0].manim_line_endpoints()?;
        let b = lines[1].manim_line_endpoints()?;
        let options = match index {
            1 => ManimAngleOptions {
                radius: Some(0.7),
                other_angle: true,
                ..Default::default()
            },
            2 => ManimAngleOptions {
                radius: Some(0.5),
                quadrant: (-1.0, 1.0),
                ..Default::default()
            },
            3 => ManimAngleOptions {
                elbow: true,
                ..Default::default()
            },
            _ => Default::default(),
        };
        let mut angle = scene.geometry(ManimGeometryOptions::angle_from_endpoints(
            [a.start, a.end],
            [b.start, b.end],
            options,
        )?)?;
        if index != 3 {
            assert!(
                (angle.angle_value(true)? - [90.0, -270.0, 270.0, 0.0, 90.0, 0.0][index]).abs()
                    < 1e-9
            );
        }
        let nominal = (index + 1) as f64 / 8.0;
        angle.set_angle_value(nominal)?;
        assert_eq!(angle.angle_value(false)?, nominal);
        scene.add_many(&[(&lines[0]).into(), (&lines[1]).into(), (&angle).into()])?;
    }
    Ok(scene)
}

pub fn program(
) -> Result<LiveProgram<crate::example_scenes::static_hold::Hold>, Box<dyn std::error::Error>> {
    Ok(crate::example_scenes::static_hold::program(scene()?, 0.2)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn paired_angle_scene_has_ordinary_leaves_without_callbacks() {
        let scene = super::scene().unwrap();
        let execution = scene.execution_session().unwrap();
        assert_eq!(execution.frame().objects.len(), 18);
        assert!(!execution.has_required_callbacks());
    }
}
