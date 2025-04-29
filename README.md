# WebGPU Compute Example

## How to Run
Simply execute `build.sh` to build and run the project.

## Architecture Overview

1. **Compute Worker Initialization**
   - A compute worker is spawned to handle WebGPU operations
   - The worker waits for data in SharedArrayBuffer (SAB)

2. **User Input Processing**
   - When user input is received, the process worker sets data in SAB
   - Notifies the compute worker through the request channel(this is also using SAB)

3. **Compute Operation**
   - Compute worker receives notification from request channel
   - Retrieves input data from SAB, performs computation
   - Returns output data to SAB

4. **Worker State**
   - After computation, compute worker returns to waiting state
   - Ready to process next input 