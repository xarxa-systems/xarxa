use xarxa_sdk::prelude::*;

#[derive(Deserialize)]
struct BookingInput {
    flight_id: String,
    user_id: String,
    amount: f64,
}

#[workflow("booking")]
fn booking_flow(input: BookingInput) -> PipelineState {
    let flight_id = input.flight_id.clone();
    let flight_id_comp = input.flight_id.clone();
    let user_id = input.user_id.clone();
    let amount = input.amount;

    Pipeline::new()
        .step("reserve_flight", move |_ctx| {
            xarxa_sdk::bindings::logging(
                format!("Reserving flight {} for user {}", flight_id, user_id).as_bytes(),
            );
            Ok(())
        })
        .compensate("reserve_flight", move |_ctx| {
            xarxa_sdk::bindings::logging(
                format!("Releasing reservation for flight {}", flight_id_comp).as_bytes(),
            );
            Ok(())
        })
        .step("charge_card", move |_ctx| {
            xarxa_sdk::bindings::logging(
                format!("Charging ${} for booking", amount).as_bytes(),
            );
            Ok(())
        })
        .step("send_confirmation", move |_ctx| {
            xarxa_sdk::bindings::logging(b"Sending confirmation email");
            Ok(())
        })
        .build()
}

xarxa_entry_point!();
