import init, { process_data } from '../pkg/wgpu_examples.js';

// Initialize WASM
await init();

// Handle messages from main thread
self.onmessage = async (e) => {
    if (e.data.type === 'process') {
        try {
            const { input_values, input_data_sab, output_data_sab, receiver_sab, sender_sab } = e.data;
            
            // Process data using WASM
            process_data(
                input_values.input1,
                input_values.input2,
                input_data_sab,
                output_data_sab,
                receiver_sab,
                sender_sab
            );

            // Notify main thread of completion
            self.postMessage({ type: 'result', result: 'Success' });
        } catch (error) {
            console.error('Error in process worker:', error);
            self.postMessage({ type: 'error', error: error.message });
        }
    }
}; 