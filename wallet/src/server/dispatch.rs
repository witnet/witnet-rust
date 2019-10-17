/// Dispatch request to the given Api instance returning a response.
#[macro_export]
macro_rules! dispatch {
    ($api:expr, $request:path => $response:path) => {{
        impl Message for $request {
            type Result = api::Result<$response>;
        }

        impl Handler<$request> for executor::Executor {
            type Result = <$request as Message>::Result;

            fn handle(&mut self, req: $request, _ctx: &mut Self::Context) -> Self::Result {
                req.handle(self.state())
            }
        }

        let api = $api.clone();
        move |params: rpc::Params| {
            let api = api.clone();
            let result = params.parse::<$request>();

            future::result(result).and_then(move |request| api.dispatch(request))
        }
    }};
}
