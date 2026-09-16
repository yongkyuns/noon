"""Pure numerical contour geometry, independently testable without Noon."""
from math import atan2, cos, isfinite, log, pi, sin, sqrt


def confidence_contour(centre, covariance, probability=.95, segments=96):
    """Closed joint contour of a 2D Gaussian; chi-square(2) = -2 log(1-p).

    Coordinates retain the caller's units. Any display normalization must be
    applied to both the mean and covariance, using the same fixed scale.
    """
    if len(centre) != 2 or len(covariance) != 2 or any(len(row) != 2 for row in covariance):
        raise ValueError('Expected a two-dimensional mean and 2x2 covariance')
    if not 0 < probability < 1 or not isinstance(segments, int) or segments < 8:
        raise ValueError('Expected 0 < probability < 1 and at least eight segments')
    if not all(isfinite(x) for x in (*centre, *covariance[0], *covariance[1])):
        raise ValueError('Mean and covariance must be finite')
    a, b, d = covariance[0][0], covariance[0][1], covariance[1][1]
    tolerance = 1e-12 * max(abs(a), abs(b), abs(d), 1e-300)
    if abs(b - covariance[1][0]) > tolerance:
        raise ValueError('Covariance must be symmetric')
    discriminant = sqrt((a - d)**2 + 4 * b*b)
    major, minor = (a + d + discriminant)/2, (a + d - discriminant)/2
    if minor < -tolerance:
        raise ValueError('Covariance must be positive semidefinite')
    angle = .5 * atan2(2*b, a - d)
    radius = sqrt(-2 * log(1 - probability))
    points = []
    for index in range(segments):
        phase = 2*pi*index/segments
        u = radius * sqrt(max(0, major)) * cos(phase)
        v = radius * sqrt(max(0, minor)) * sin(phase)
        points.append((centre[0] + u*cos(angle) - v*sin(angle),
                       centre[1] + u*sin(angle) + v*cos(angle)))
    return points + [points[0]]
