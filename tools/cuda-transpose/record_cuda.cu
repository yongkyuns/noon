// Instrumented adaptation of CUDA Algorithm Lab's shared/padded transpose.
// Original author: Arash (ourarash). Reproduced with Noon for demonstration.
// MIT notice: ../../web/python/examples/cuda_transpose_banks.LICENSE.
// Original: https://github.com/ourarash/cuda-algorithm-lab/tree/main/matrix_transpose
//
// nvcc -std=c++17 -O2 record_cuda.cu -o record_cuda
// ./record_cuda 3 > gpu_trace.py
// Replace GPU_TRACE=None in the Noon source with the printed assignment.
//
// This runs CUDA. It records source-level addresses and data, NOT an instruction
// timeline, bank-service order, or benchmark. Instrumentation changes execution.
#include <cuda_runtime.h>
#include <array>
#include <cstddef>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

constexpr int TILE_DIM = 32;
constexpr int BLOCK_ROWS = 8;
constexpr int N = TILE_DIM * TILE_DIM;
struct Access { int row, col; unsigned word; int global_index; float value; };

// PAD is the only algorithmic difference; access logging is instrumentation.
template<int PAD>
__global__ void transpose_recorded(const float *in, float *out, int rows, int cols,
                                   Access *loads, Access *reads) {
  __shared__ float tile[TILE_DIM][TILE_DIM + PAD];
  int x = blockIdx.x * TILE_DIM + threadIdx.x;
  int y = blockIdx.y * TILE_DIM + threadIdx.y;
  for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {
    if (x < cols && y + j < rows) {
      tile[threadIdx.y + j][threadIdx.x] = in[static_cast<size_t>(y + j) * cols + x];
      int r = threadIdx.y + j, c = threadIdx.x;
      loads[r * TILE_DIM + c] = {r, c,
        static_cast<unsigned>(__cvta_generic_to_shared(&tile[r][c]) / sizeof(float)),
        (y + j) * cols + x, tile[r][c]};
    }
  }
  __syncthreads();
  x = blockIdx.y * TILE_DIM + threadIdx.x;
  y = blockIdx.x * TILE_DIM + threadIdx.y;
  for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {
    if (x < rows && y + j < cols) {
      out[static_cast<size_t>(y + j) * rows + x] = tile[threadIdx.x][threadIdx.y + j];
      int r = threadIdx.x, c = threadIdx.y + j;
      reads[c * TILE_DIM + r] = {r, c,
        static_cast<unsigned>(__cvta_generic_to_shared(&tile[r][c]) / sizeof(float)),
        (y + j) * rows + x, out[static_cast<size_t>(y + j) * rows + x]};
    }
  }
}

void check(cudaError_t error, const char *operation) {
  if (error != cudaSuccess)
    throw std::runtime_error(std::string(operation) + ": " + cudaGetErrorString(error));
}
template<class T> class DeviceBuffer {
 public:
  T *ptr = nullptr;
  DeviceBuffer() { check(cudaMalloc(reinterpret_cast<void **>(&ptr), N * sizeof(T)), "cudaMalloc"); }
  ~DeviceBuffer() { if (ptr) cudaFree(ptr); }
  DeviceBuffer(const DeviceBuffer&) = delete;
  DeviceBuffer& operator=(const DeviceBuffer&) = delete;
};
struct Result {
  int stride;
  unsigned base_word;
  std::array<int, 4> counts{};
  std::vector<Access> selected;
  std::vector<float> output;
};

template<int PAD> Result run(int column) {
  DeviceBuffer<float> input, output;
  DeviceBuffer<Access> loads, reads;
  std::vector<float> source(N);
  for (int i = 0; i < N; ++i) source[i] = static_cast<float>(i);
  check(cudaMemcpy(input.ptr, source.data(), N * sizeof(float), cudaMemcpyHostToDevice), "upload input");
  // Sentinels make missing writes detectable during validation.
  check(cudaMemset(output.ptr, 0xff, N * sizeof(float)), "initialize output");
  check(cudaMemset(loads.ptr, 0xff, N * sizeof(Access)), "initialize loads");
  check(cudaMemset(reads.ptr, 0xff, N * sizeof(Access)), "initialize reads");
  transpose_recorded<PAD><<<dim3(1, 1), dim3(TILE_DIM, BLOCK_ROWS)>>>(
      input.ptr, output.ptr, TILE_DIM, TILE_DIM, loads.ptr, reads.ptr);
  check(cudaGetLastError(), "launch transpose");
  check(cudaDeviceSynchronize(), "execute transpose");
  std::vector<Access> hl(N), hr(N);
  Result result{TILE_DIM + PAD, 0, {}, {}, std::vector<float>(N)};
  check(cudaMemcpy(hl.data(), loads.ptr, N * sizeof(Access), cudaMemcpyDeviceToHost), "read loads");
  check(cudaMemcpy(hr.data(), reads.ptr, N * sizeof(Access), cudaMemcpyDeviceToHost), "read reads");
  check(cudaMemcpy(result.output.data(), output.ptr, N * sizeof(float), cudaMemcpyDeviceToHost), "read output");
  result.base_word = hl[0].word;
  for (int r = 0; r < TILE_DIM; ++r) {
    for (int c = 0; c < TILE_DIM; ++c) {
      const auto &load = hl[r * TILE_DIM + c];
      const auto &read = hr[c * TILE_DIM + r];
      unsigned word = result.base_word + r * result.stride + c;
      float value = static_cast<float>(r * TILE_DIM + c);
      if (load.row != r || load.col != c || load.word != word || load.value != value ||
          load.global_index != r * TILE_DIM + c || read.row != r || read.col != c ||
          read.word != word || read.global_index != c * TILE_DIM + r || read.value != value ||
          result.output[c * TILE_DIM + r] != value)
        throw std::runtime_error("CUDA address/value/output validation failed");
      ++result.counts[r / BLOCK_ROWS];
    }
  }
  for (int lane = 0; lane < TILE_DIM; ++lane) result.selected.push_back(hr[column * TILE_DIM + lane]);
  return result;
}

void print_run(const Result &r) {
  std::cout << "{\"stride\":" << r.stride << ",\"base_word\":" << r.base_word << ",\"loads\":[";
  for (int i = 0; i < 4; ++i) std::cout << (i ? "," : "") << r.counts[i];
  std::cout << "],\"verified_output_words\":1024,\"reads\":[";
  for (std::size_t i = 0; i < r.selected.size(); ++i) {
    const auto &a = r.selected[i];
    std::cout << (i ? "," : "") << '[' << a.row << ',' << a.col << ',' << a.word << ','
              << a.global_index << ',' << static_cast<int>(a.value) << ']';
  }
  std::cout << "]}";
}

int main(int argc, char **argv) {
  try {
    if (argc > 2) throw std::runtime_error("Usage: record_cuda [column:0..31]");
    int column = 3;
    if (argc == 2) {
      std::size_t end = 0;
      column = std::stoi(argv[1], &end);
      if (end != std::string(argv[1]).size()) throw std::runtime_error("Invalid column");
    }
    if (column < 0 || column >= TILE_DIM) throw std::runtime_error("Column must be 0..31");
    cudaDeviceProp device{};
    check(cudaGetDeviceProperties(&device, 0), "find CUDA device");
    if (device.major < 5) throw std::runtime_error("This demonstration requires compute capability >= 5.0");
    check(cudaSetDevice(0), "select CUDA device");
    auto unpadded = run<0>(column);
    auto padded = run<1>(column);
    if (unpadded.output != padded.output) throw std::runtime_error("Padded/unpadded outputs differ");
    std::cerr << device.name << ": validated both 1024-word outputs and all recorded accesses.\n"
              << "Addresses/values only; no timing or bank-service-order measurement.\n";
    // Print only after BOTH runs validate; stdout is directly pasteable Python.
    std::cout << "GPU_TRACE = {\"column\":" << column << ",\"runs\":[\n";
    print_run(unpadded);
    std::cout << ",\n";
    print_run(padded);
    std::cout << "\n]}\n";
    return 0;
  } catch (const std::exception &e) {
    std::cerr << "ERROR: " << e.what() << '\n';
    return 1;
  }
}
