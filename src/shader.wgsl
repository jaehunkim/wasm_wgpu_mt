struct Input {
    data: array<u32>,
};

struct Output {
    data: array<u32>,
};

@group(0) @binding(0)
var<storage, read> input: Input;

@group(0) @binding(1)
var<storage, read_write> output: Output;

@compute @workgroup_size(1)
fn main() {
    let a = input.data[1];
    let b = input.data[2];
    output.data[3] = a + b;
} 