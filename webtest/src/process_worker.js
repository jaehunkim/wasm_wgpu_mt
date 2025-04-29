import init, { process_data } from '../pkg/wgpu_examples.js';

// Handle messages from main thread
self.onmessage = async (e) => {
    if (e.data.type === 'init') {
        await init();
        console.log('Process worker initialized');
    }
    if (e.data.type === 'process') {
        try {
            const { input_values, input_data_sab, output_data_sab, receiver_sab, sender_sab } = e.data;
            
            console.log('Processing data in process worker');
            console.log(input_values);
            // Process data using WASM
            process_data(
                input_values.input1,
                input_values.input2,
                input_data_sab,
                output_data_sab,
                receiver_sab,
                sender_sab
            );
        } catch (error) {
        }
    }
}; 