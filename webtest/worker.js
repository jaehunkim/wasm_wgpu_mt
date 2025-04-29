import init, { run } from './pkg/wgpu_examples.js';

// Create SharedArrayBuffer
const inputData = new SharedArrayBuffer(1024); // 256 u32 values
const outputData = new SharedArrayBuffer(1024); // 256 u32 values
const receiverState = new SharedArrayBuffer(4); // 1 u32 value
const senderState = new SharedArrayBuffer(4); // 1 u32 value

// Convert SharedArrayBuffer to Int32Array
const inputArray = new Int32Array(inputData);
const outputArray = new Int32Array(outputData);
const receiverArray = new Int32Array(receiverState);
const senderArray = new Int32Array(senderState);

// Initialize input data
for (let i = 0; i < inputArray.length; i++) {
    inputArray[i] = i;
}

// Initialize states
receiverArray[0] = 0;
senderArray[0] = 0;

// Handle web worker messages
onmessage = async (e) => {
    if (e.data === 'start') {
        try {
            // Initialize WebAssembly module
            await init();

            // Execute WebGPU work
            await run(inputData, outputData, receiverState, senderState);

            // Print results after work completion
            console.log('Input:', Array.from(inputArray));
            console.log('Output:', Array.from(outputArray));

            // Send completion message to main thread
            postMessage('done');
        } catch (error) {
            console.error('Error:', error);
            postMessage('error');
        }
    }
}; 