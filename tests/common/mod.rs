macro_rules! rt_exec {
    ($docker_call:expr, $assertions:expr) => {{
        let mut rt = Runtime::new().unwrap();
        let call = $docker_call;
        $assertions(
            rt.block_on(call)
                .or_else(|e| {
                    println!("{}", e);
                    Err(e)
                }).unwrap(),
        );
        rt.shutdown_now().wait().unwrap();
    }};
}

macro_rules! rt_stream {
    ($docker_call:expr, $assertions:expr) => {{
        let mut rt = Runtime::new().unwrap();
        let call = $docker_call.fold(vec![], |mut v, line| {
            v.push(line);
            future::ok::<_, Error>(v)
        });
        $assertions(
            rt.block_on(call)
                .or_else(|e| {
                    println!("{}", e);
                    Err(e)
                }).unwrap(),
        );
        rt.shutdown_now().wait().unwrap();
    }};
}

macro_rules! rt_exec_ignore_error {
    ($docker_call:expr, $assertions:expr) => {{
        let mut rt = Runtime::new().unwrap();
        let call = $docker_call;
        $assertions(rt.block_on(call).unwrap_or_else(|_| ()));
        rt.shutdown_now().wait().unwrap();
    }};
}

macro_rules! connect_to_docker_and_run {
    ($exec:expr) => {{
        #[cfg(unix)]
        $exec(Docker::connect_with_unix_defaults().unwrap());
        #[cfg(test_http)]
        $exec(Docker::connect_with_http_defaults().unwrap());
        #[cfg(ssl)]
        $exec(Docker::connect_with_ssl_defaults());
        #[cfg(windows)]
        $exec(Docker::connect_with_named_pipe_defaults());
    }};
}
