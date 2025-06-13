use std::{path::PathBuf, time::Duration};

use crossbeam_channel::{RecvError, select, unbounded};

use loss72_platemaker_core::{fs::File, log, model::GenerationContext};
use loss72_platemaker_structure::{ArticleFile, AssetFile};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, new_debouncer};

use crate::{
    build_tasks::{
        build_files, copy_individual_assets_files, copy_individual_template_files, run_all_build_steps,
    },
    config::Configuration,
    error::{report_error, report_if_fail},
};

#[derive(Debug)]
pub struct WatchParam {
    pub build_first: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum WatcherError {
    #[error("Error trying watching file system: {0}")]
    NotifierError(#[from] notify::Error),
}

#[derive(Debug)]
pub enum Changed {
    Article(PathBuf),
    Template,
}

pub fn watch_for_change(config: &Configuration, param: &WatchParam, ctx: &GenerationContext) -> Result<(), WatcherError> {
    if param.build_first {
        log!(ok: "--build-first specified - full building first!");
        report_if_fail(|| run_all_build_steps(config, ctx)).ok();
        log!(ok: "Full building completed, now starting watch...");
    }

    let (md_tx, md_rx) = unbounded();
    let (tpl_tx, tpl_rx) = unbounded();
    let (ctrlc_tx, ctrlc_rx) = unbounded::<()>();

    let mut markdown_watcher = new_debouncer(Duration::from_millis(500), None, md_tx)?;
    markdown_watcher.watch(config.article_md_dir.path(), RecursiveMode::Recursive)?;

    let mut template_watcher = new_debouncer(Duration::from_millis(500), None, tpl_tx)?;
    template_watcher.watch(config.html_template_dir.path(), RecursiveMode::Recursive)?;

    if let Err(e) = ctrlc::set_handler(move || {
        ctrlc_tx.send(()).ok();
    }) {
        log!(warn: "Ctrl+C Handler could not be set.");
        log!(warn: "{}", e);
    }

    log!(job_start: "Platemaker is watching for the changes!");
    log!(section: "Enter Ctrl-C to end watching.");
    log!(section: "Configurations");
    log!(step: "   Article folder: {}", config.article_md_dir.path().display());
    log!(step: "  Template folder: {}", config.html_template_dir.path().display());
    log!(ok: "Changes to the files in directories above will be watched");

    loop {
        select! {
            recv(md_rx) -> received => {
                let Some(files) = handle_notify_event(received) else {
                    continue;
                };

                let articles = files.iter()
                    .filter_map(|file| ArticleFile::from_file(file, &config.article_md_dir))
                    .collect::<Vec<_>>();

                build_files(config, &articles, false, ctx)
                    .inspect_err(report_error)
                    .ok();

                let article_asset_file = files.iter()
                    .filter_map(|file| AssetFile::from_file(file, &config.article_md_dir))
                    .collect::<Vec<_>>();

                copy_individual_assets_files(config, &article_asset_file)
                    .inspect_err(report_error)
                    .ok();
            },
            recv(tpl_rx) -> received => {
                let Some(files) = handle_notify_event(received) else {
                    continue;
                };

                copy_individual_template_files(config, &files, ctx)
                    .inspect_err(report_error)
                    .ok();
            },
            recv(ctrlc_rx) -> _ => {
                println!();
                log!(job_end: "Receved Ctrl-C, Exiting!");
                break;
            }
        }
    }

    Ok(())
}

fn handle_notify_event(received: Result<DebounceEventResult, RecvError>) -> Option<Vec<File>> {
    let events = match received {
        Ok(Ok(events)) => events,
        Ok(Err(errors)) => {
            println!("warning: filesystem seems to be changed but the detail could not be read");
            errors.iter().for_each(|error| {
                println!("         - {error}");
            });
            return None;
        }
        Err(error) => {
            println!("warning: filesystem seems to be changed but the detail could not be read");
            println!("         {error}");
            return None;
        }
    };

    Some(
        events
            .iter()
            .flat_map(|event| match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => event.paths.clone(),
                _ => vec![],
            })
            .filter(|path| path.exists())
            .filter_map(|file| match File::new(file) {
                Ok(file) => Some(file),
                Err(error) => {
                    log!(warn: "There was an error during checking what changed: {}", error);
                    None
                }
            })
            .collect(),
    )
}
