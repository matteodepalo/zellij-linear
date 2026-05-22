//! `zellij-linear init` — pick a Linear project and write `./.linear.toml`.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use linear_client::auth::load as load_auth;
use linear_client::http::{HttpClient, HttpResponse, HttpVerb};
use linear_client::queries::Q_PROJECTS;
use linear_client::types::{GraphQLResponse, Project, ProjectsRoot};
use linear_client::LINEAR_GRAPHQL;

use crate::http_impl::ReqwestClient;

const CONFIG_FILE: &str = ".linear.toml";

pub fn run(project: Option<String>, force: bool) -> Result<()> {
    let dest = PathBuf::from(CONFIG_FILE);
    if dest.exists() && !force {
        bail!(
            "{} already exists. Re-run with --force to overwrite.",
            dest.display()
        );
    }

    let auth = load_auth().context(
        "not logged in (run `zellij-linear configure --client-id …` and `zellij-linear login`)",
    )?;
    let http = ReqwestClient::new().context("constructing HTTP client")?;
    let projects = fetch_projects(&http, &auth.access_token)?;
    if projects.is_empty() {
        bail!("no projects visible to {} on Linear", auth.user_email);
    }

    let chosen = match project {
        Some(query) => resolve_project(&projects, &query)?,
        None => pick_interactively(&projects)?,
    };

    write_config(&dest, &chosen)?;
    eprintln!(
        "Wrote {}\n  project_id   = \"{}\"\n  project_name = \"{}\"",
        dest.display(),
        chosen.id,
        chosen.name,
    );
    Ok(())
}

fn fetch_projects(http: &dyn HttpClient, access_token: &str) -> Result<Vec<Project>> {
    let body = serde_json::to_vec(&serde_json::json!({ "query": Q_PROJECTS }))?;
    let auth_header = format!("Bearer {access_token}");
    let resp: HttpResponse = http.request(
        LINEAR_GRAPHQL,
        HttpVerb::Post,
        &[
            ("Authorization", auth_header.as_str()),
            ("Content-Type", "application/json"),
        ],
        &body,
    )?;
    if !resp.is_success() {
        bail!(
            "fetching projects failed: status {}: {}",
            resp.status,
            resp.body_as_str()
        );
    }
    let parsed: GraphQLResponse<ProjectsRoot> = serde_json::from_slice(&resp.body)?;
    if !parsed.errors.is_empty() {
        bail!(
            "Linear GraphQL errors: {}",
            parsed
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let root = parsed
        .data
        .ok_or_else(|| anyhow!("projects query returned no data"))?;
    Ok(root.projects.nodes)
}

fn resolve_project(projects: &[Project], query: &str) -> Result<Project> {
    // Exact UUID match wins.
    if let Some(hit) = projects.iter().find(|p| p.id == query) {
        return Ok(hit.clone());
    }
    let needle = query.to_lowercase();
    let case_insensitive_exact: Vec<&Project> = projects
        .iter()
        .filter(|p| p.name.to_lowercase() == needle)
        .collect();
    if case_insensitive_exact.len() == 1 {
        return Ok(case_insensitive_exact[0].clone());
    }
    let candidates: Vec<&Project> = projects
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&needle))
        .collect();
    match candidates.len() {
        0 => bail!("no project matching `{query}`"),
        1 => Ok(candidates[0].clone()),
        _ => {
            let names: Vec<&str> = candidates.iter().map(|p| p.name.as_str()).collect();
            bail!("`{query}` matches multiple projects: {}", names.join(", "));
        }
    }
}

fn pick_interactively(projects: &[Project]) -> Result<Project> {
    if !io::stdin().is_terminal() {
        bail!("stdin is not a TTY — pass --project=<NAME|UUID> when running non-interactively");
    }
    eprintln!("Projects visible on Linear:\n");
    let name_width = projects.iter().map(|p| p.name.len()).max().unwrap_or(0);
    for (i, p) in projects.iter().enumerate() {
        let team = p.teams.nodes.first().map(|t| t.key.as_str()).unwrap_or("");
        eprintln!(
            "  {:>2}) {:<width$}  ({})",
            i + 1,
            p.name,
            team,
            width = name_width
        );
    }
    eprintln!();

    let mut line = String::new();
    loop {
        line.clear();
        eprint!("Pick a project (1-{}): ", projects.len());
        io::stderr().flush().ok();
        if io::stdin().lock().read_line(&mut line)? == 0 {
            bail!("stdin closed before a selection was made");
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed.parse::<usize>() {
            Ok(n) if (1..=projects.len()).contains(&n) => {
                return Ok(projects[n - 1].clone());
            }
            _ => eprintln!("Enter a number between 1 and {}.", projects.len()),
        }
    }
}

fn write_config(dest: &PathBuf, project: &Project) -> Result<()> {
    let contents = format!(
        "# Generated by `zellij-linear init`. See examples/.linear.toml in\n\
         # the zellij-linear repo for the full schema (filter states, Claude\n\
         # target command, prompt template, etc.).\n\
         project_id = \"{}\"\n\
         project_name = \"{}\"\n",
        project.id, project.name
    );
    std::fs::write(dest, contents).with_context(|| format!("writing {}", dest.display()))?;
    Ok(())
}
