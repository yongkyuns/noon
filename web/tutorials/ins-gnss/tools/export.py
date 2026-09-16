"""Export the exact teaching experiment without Noon, NumPy, or a browser."""
import argparse
import csv
from dataclasses import asdict
from hashlib import sha256
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
from model import Experiment, metrics, simulate


def export(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    config = Experiment()
    samples = simulate(config)
    columns = ('time_s', 'truth_position_m', 'truth_velocity_m_s',
               'inertial_position_m', 'estimated_position_m',
               'estimated_velocity_m_s', 'physical_bias_m_s2',
               'position_sigma_m', 'observation_m', 'innovation_m',
               'innovation_variance_m2', 'accepted', 'prior_position_m')
    covariance_columns = tuple(f'P_{i}{j}' for i in range(3) for j in range(3))
    with (destination / 'trace.csv').open('w', newline='') as stream:
        writer = csv.writer(stream)
        writer.writerow(columns + covariance_columns)
        for sample in samples:
            values = (sample.time, sample.truth, sample.truth_velocity,
                      sample.inertial, sample.position, sample.velocity,
                      sample.bias, sample.sigma, sample.observation,
                      sample.innovation, sample.innovation_variance,
                      sample.accepted, sample.prior_position)
            writer.writerow(values + tuple(value for row in sample.covariance for value in row))
    updates = [{
        'time_s': sample.time,
        'prior_state': sample.prior_state,
        'prior_covariance': sample.prior_covariance,
        'observation_m': sample.observation,
        'measurement_variance_m2': config.position_sigma**2,
        'innovation_m': sample.innovation,
        'innovation_variance_m2': sample.innovation_variance,
        'candidate_gain': sample.gain,
        'injected_correction': sample.correction,
        'posterior_state': sample.state,
        'posterior_covariance': sample.covariance,
        'accepted': sample.accepted,
        'evaluation_truth_only': (sample.truth, sample.truth_velocity, config.bias),
    } for sample in samples if sample.prior_state is not None]
    (destination / 'updates.json').write_text(json.dumps(updates, indent=2) + '\n')
    manifest = {
        'model': '1D position/velocity/physical-acceleration-bias Kalman filter',
        'scope': 'Teaching simulation, not the 24-error-state ECEF reference',
        'model_sha256': sha256((ROOT / 'src/model.py').read_bytes()).hexdigest(),
        'config': asdict(config),
        'metrics': metrics(samples, config),
        'covariance_order': ['position_m', 'velocity_m_s', 'physical_bias_m_s2'],
        'noise_convention': 'Independent acceleration sample standard deviation, not a PSD',
        'truth_use': 'Sensor generation and evaluation only; declared initial means are zero',
        'worked_update_time_s': config.outage_end,
        'updates_sha256': sha256((destination / 'updates.json').read_bytes()).hexdigest(),
        'trace_sha256': sha256((destination / 'trace.csv').read_bytes()).hexdigest(),
    }
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest['metrics'], indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path)
    export(parser.parse_args().destination)
