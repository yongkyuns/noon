"""One scene entry point. The browser supplies a chapter number in context."""
from noon import Scene


class InsGnssTutorial(Scene):
    async def construct(self):
        lessons = (lesson_fusion,lesson_drift,lesson_sensors,lesson_frames,
                   lesson_mechanization,lesson_scalar,lesson_covariance,lesson_error_state,
                   lesson_prediction,lesson_update,lesson_vehicle,lesson_calibration,
                   lesson_robustness,lesson_reference)
        chapter = int(context.get('chapter',1))
        if not 1 <= chapter <= len(lessons):
            raise ValueError('Unknown tutorial chapter')
        await lessons[chapter-1](self)
