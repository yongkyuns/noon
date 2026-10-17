// CUDA Algorithm Lab by Arash (ourarash), reproduced with Noon for demonstration.
// The two kernel function bodies are unchanged upstream excerpts; harness/main
// code is omitted. Constants are supplied by record_cuda.cu.
// MIT notice: ../../web/python/examples/cuda_transpose_banks.LICENSE
// Upstream original blobs (Git SHA-1):
// 02_shared_memory/02_transpose_shared_memory.cu: 26dcf981ae7d98194e0c4a8923f0e4966b21b5f8
// 03_padded/03_transpose_padded.cu: d894a8330813e793c8156054fd0bcf884569323a
// https://github.com/ourarash/cuda-algorithm-lab/tree/main/matrix_transpose
#pragma once

__global__ void transpose_shared(const float *in, float *out, int rows, int cols) {
  __shared__ float tile[TILE_DIM][TILE_DIM];

  int x = blockIdx.x * TILE_DIM + threadIdx.x;
  int y = blockIdx.y * TILE_DIM + threadIdx.y;
  for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {
    if (x < cols && y + j < rows) {
      tile[threadIdx.y + j][threadIdx.x] = in[static_cast<size_t>(y + j) * cols + x];
    }
  }
  __syncthreads();

  // The output block sits at the mirrored position of the input block.
  x = blockIdx.y * TILE_DIM + threadIdx.x;
  y = blockIdx.x * TILE_DIM + threadIdx.y;
  for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {
    if (x < rows && y + j < cols) {
      out[static_cast<size_t>(y + j) * rows + x] = tile[threadIdx.x][threadIdx.y + j];
    }
  }
}

__global__ void transpose_padded(const float *in, float *out, int rows, int cols) {
  __shared__ float tile[TILE_DIM][TILE_DIM + 1];

  int x = blockIdx.x * TILE_DIM + threadIdx.x;
  int y = blockIdx.y * TILE_DIM + threadIdx.y;
  for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {
    if (x < cols && y + j < rows) {
      tile[threadIdx.y + j][threadIdx.x] = in[static_cast<size_t>(y + j) * cols + x];
    }
  }
  __syncthreads();

  x = blockIdx.y * TILE_DIM + threadIdx.x;
  y = blockIdx.x * TILE_DIM + threadIdx.y;
  for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {
    if (x < rows && y + j < cols) {
      out[static_cast<size_t>(y + j) * rows + x] = tile[threadIdx.x][threadIdx.y + j];
    }
  }
}
