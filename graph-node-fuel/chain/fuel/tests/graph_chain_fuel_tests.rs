#[cfg(test)]
mod tests {
    use fuel_vm::fuel_tx::Transaction;
    use prost::Message;
    use graph_chain_fuel::codec::{};

    // #[test]
    // fn test_grpc() {
    //
    //     // Create a MyRequest message
    //     let my_request = MyRequest {
    //         name: "John".to_string(),
    //     };
    //
    //     // Serialize the message
    //     let serialized_data = my_request.encode_to_vec();
    //
    //     // Deserialize the message
    //     let deserialized_request = MyRequest::decode(serialized_data.as_slice()).unwrap();
    //
    //     // Print the deserialized message
    //     println!("Deserialized MyRequest: {:?}", deserialized_request);
    //
    //     // Create a MyResponse message
    //     let my_response = MyResponse {
    //         greeting: "Hello, John!".to_string(),
    //     };
    //
    //     // Serialize the message
    //     let serialized_response_data = my_response.encode_to_vec();
    //
    //     // Deserialize the message
    //     let deserialized_response = MyResponse::decode(serialized_response_data.as_slice()).unwrap();
    //
    //     // Print the deserialized message
    //     println!("Deserialized MyResponse: {:?}", deserialized_response);
    // }

    #[test]
    fn test_fuel_types() {

        let transaction = Transaction::Script(Default::default());



    }
}