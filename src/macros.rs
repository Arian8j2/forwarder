macro_rules! loop_select {
    ($($tokens:tt)+) => {
        loop {
            tokio::select! {
                $($tokens)+
            }
        }
    };
}

pub(crate) use loop_select;
