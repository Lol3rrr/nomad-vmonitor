use std::collections::HashMap;

#[derive(Debug)]
pub struct Metrics {
    up_to_date: prometheus::GaugeVec,
    out_of_date: prometheus::GaugeVec,
}

#[derive(Debug)]
pub enum UpdatedVersion {
    UpToDate { version: String },
    OutOfDate { current: String, newest: String },
}

impl Metrics {
    pub fn new(reg: &prometheus::Registry) -> Self {
        let uptodate = prometheus::GaugeVec::new(
            prometheus::Opts::new(
                "up_to_date",
                "The Jobs/Tasks that are up to date will be set to 1 others to 0",
            ),
            &["job", "group", "task"],
        )
        .unwrap();

        let out_of_date = prometheus::GaugeVec::new(
            prometheus::Opts::new(
                "out_of_date",
                "The Jobs/Tasks that are out of date will be set to 0 others to 1",
            ),
            &["job", "group", "task"],
        )
        .unwrap();

        reg.register(Box::new(uptodate.clone())).unwrap();
        reg.register(Box::new(out_of_date.clone())).unwrap();

        Self {
            up_to_date: uptodate,
            out_of_date,
        }
    }

    pub fn update(&self, job: &str, group: &str, task: &str, version: UpdatedVersion) {
        let labels = [("job", job), ("group", group), ("task", task)]
            .into_iter()
            .collect::<HashMap<&str, &str>>();

        let uptodate_metric = self.up_to_date.get_metric_with(&labels).unwrap();
        let outofdate_metric = self.out_of_date.get_metric_with(&labels).unwrap();

        match version {
            UpdatedVersion::UpToDate { version } => {
                uptodate_metric.set(1.0);
                outofdate_metric.set(0.0);
            }
            UpdatedVersion::OutOfDate { current, newest } => {
                uptodate_metric.set(0.0);
                outofdate_metric.set(1.0);
            }
        };
    }
}