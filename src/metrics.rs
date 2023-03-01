use std::collections::HashMap;

#[derive(Debug)]
pub struct Metrics {
    up_to_date: prometheus::GaugeVec,
    out_of_date: prometheus::GaugeVec,
    versions: prometheus::GaugeVec,
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

        let versions = prometheus::GaugeVec::new(
            prometheus::Opts::new("versions", "The Versions for the Jobs/Tasks"),
            &["job", "group", "task", "current", "newest"],
        )
        .unwrap();

        reg.register(Box::new(uptodate.clone())).unwrap();
        reg.register(Box::new(out_of_date.clone())).unwrap();
        reg.register(Box::new(versions.clone())).unwrap();

        Self {
            up_to_date: uptodate,
            out_of_date,
            versions,
        }
    }

    pub fn clear(&self) {
        self.out_of_date.reset();
        self.up_to_date.reset();
        self.versions.reset();
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

                self.versions
                    .get_metric_with(
                        &[
                            ("job", job),
                            ("group", group),
                            ("task", task),
                            ("current", &version),
                            ("newest", &version),
                        ]
                        .into_iter()
                        .collect(),
                    )
                    .unwrap()
                    .set(1.0);
            }
            UpdatedVersion::OutOfDate { current, newest } => {
                uptodate_metric.set(0.0);
                outofdate_metric.set(1.0);

                self.versions
                    .get_metric_with(
                        &[
                            ("job", job),
                            ("group", group),
                            ("task", task),
                            ("current", &current),
                            ("newest", &newest),
                        ]
                        .into_iter()
                        .collect(),
                    )
                    .unwrap()
                    .set(1.0);
            }
        };
    }
}
