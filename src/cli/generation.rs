fn run_completions(args: CompletionsArgs) -> Result<i32> {
    let mut command = Cli::command();
    let mut stdout = std::io::stdout().lock();
    match args.shell {
        CompletionShell::Bash => generate(Shell::Bash, &mut command, PROJECT_NAME, &mut stdout),
        CompletionShell::Zsh => generate(Shell::Zsh, &mut command, PROJECT_NAME, &mut stdout),
        CompletionShell::Fish => generate(Shell::Fish, &mut command, PROJECT_NAME, &mut stdout),
        CompletionShell::Powershell => {
            generate(Shell::PowerShell, &mut command, PROJECT_NAME, &mut stdout)
        }
        CompletionShell::Nushell => generate(
            clap_complete_nushell::Nushell,
            &mut command,
            PROJECT_NAME,
            &mut stdout,
        ),
    }
    Ok(0)
}

fn write_generated_output(output: Option<&Path>, bytes: &[u8]) -> Result<()> {
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    } else {
        use std::io::Write;
        std::io::stdout().lock().write_all(bytes)?;
    }
    Ok(())
}

fn run_man(args: ManArgs) -> Result<i32> {
    let mut bytes = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut bytes)?;
    let rendered = String::from_utf8(bytes).context("generated manual was not UTF-8")?;
    let normalized = format!(
        "{}\n",
        rendered
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    );
    write_generated_output(args.output.as_deref(), normalized.as_bytes())?;
    Ok(0)
}

fn markdown_command(command: &clap::Command, path: &str, output: &mut String) {
    output.push_str(&format!("## `{path}`\n\n"));
    if let Some(about) = command.get_about() {
        output.push_str(&format!("{about}\n\n"));
    }
    let arguments = command.get_arguments().collect::<Vec<_>>();
    if !arguments.is_empty() {
        output.push_str("| Argument | Description |\n| --- | --- |\n");
        for argument in arguments {
            let name = argument
                .get_long()
                .map(|value| format!("--{value}"))
                .unwrap_or_else(|| argument.get_id().to_string());
            let help = argument
                .get_help()
                .map(ToString::to_string)
                .unwrap_or_default();
            output.push_str(&format!("| `{name}` | {} |\n", help.replace('|', "\\|")));
        }
        output.push('\n');
    }
    for subcommand in command.get_subcommands() {
        markdown_command(
            subcommand,
            &format!("{path} {}", subcommand.get_name()),
            output,
        );
    }
}

fn run_reference(args: ReferenceArgs) -> Result<i32> {
    let command = Cli::command();
    let mut markdown =
        "# Git Slop CLI Reference\n\nGenerated from the live Clap command tree.\n\n".to_string();
    markdown_command(&command, "git-slop", &mut markdown);
    let normalized = format!("{}\n", markdown.trim_end());
    write_generated_output(args.output.as_deref(), normalized.as_bytes())?;
    Ok(0)
}

fn run_html(repo_root: &Path, args: HtmlArgs) -> Result<i32> {
    let explicit_report = args.report.clone();
    let (loaded, report_path) = report_or_missing(repo_root, args.report.as_deref())?;
    let output = args.output.unwrap_or_else(|| {
        explicit_report
            .as_ref()
            .and_then(|path| path.parent())
            .map_or_else(|| config::latest_dir(repo_root), Path::to_path_buf)
            .join("report.html")
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let bounded = |pointer: &str, limit: usize| {
        loaded
            .pointer(pointer)
            .and_then(Value::as_array)
            .map(|records| records.iter().take(limit).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let bounded_sections = |pointer: &str, limit: usize| {
        loaded
            .pointer(pointer)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|sections| sections.values())
            .filter_map(Value::as_array)
            .flatten()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
    };
    let embedded_limit = 5_000usize;
    let source_report = relative_display(&report_path, repo_root);
    let payload = serde_json::to_string(&json!({
        "schema_version": loaded.get("schema_version"),
        "generated_at": loaded.get("generated_at"),
        "analyzed_revision_at": loaded.get("analyzed_revision_at"),
        "analyzer": loaded.get("analyzer"),
        "repo": loaded.get("repo"),
        "scope": loaded.get("scope"),
        "config_digests": {
            "config": loaded.pointer("/analyzer/config_digest"),
            "analysis": loaded.pointer("/analyzer/analysis_config_digest"),
            "evidence": loaded.pointer("/analyzer/evidence_config_digest"),
            "policy": loaded.pointer("/analyzer/policy_config_digest"),
            "presentation": loaded.pointer("/analyzer/presentation_config_digest")
        },
        "collection_metadata": loaded.get("collection_metadata"),
        "evidence_completeness": loaded.get("evidence_completeness"),
        "files": bounded("/files", embedded_limit),
        "folders": bounded("/folders", embedded_limit),
        "action_queue": bounded("/action_queue", embedded_limit),
        "health": {
            "summary": loaded.pointer("/health/summary"),
            "findings": bounded("/health/findings", embedded_limit)
        },
        "organization": {
            "relationships": bounded_sections("/overlays/organization_health/relationships", embedded_limit),
            "clusters": bounded_sections("/overlays/organization_health/clusters", embedded_limit)
        },
        "embedded_evidence": {
            "record_limit_per_view": embedded_limit,
            "truncated_views": {
                "files": loaded.pointer("/files").and_then(Value::as_array).is_some_and(|values| values.len() > embedded_limit),
                "folders": loaded.pointer("/folders").and_then(Value::as_array).is_some_and(|values| values.len() > embedded_limit),
                "action_queue": loaded.pointer("/action_queue").and_then(Value::as_array).is_some_and(|values| values.len() > embedded_limit),
                "health": loaded.pointer("/health/findings").and_then(Value::as_array).is_some_and(|values| values.len() > embedded_limit)
            }
        },
        "source_report": source_report
    }))?
    .replace("</", "<\\/");
    let csp_nonce = &hex::encode(sha2::Sha256::digest(payload.as_bytes()))[..24];
    let html = format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'nonce-{csp_nonce}'; script-src 'nonce-{csp_nonce}'; base-uri 'none'; form-action 'none'"><title>Git Slop local report</title><style nonce="{csp_nonce}">
:root {{ color-scheme: light dark; font: 15px system-ui,sans-serif }} body {{ margin: 2rem; max-width: 1100px }}
input,select {{ padding:.55rem; margin:0 .5rem .75rem 0 }} table {{ width:100%; border-collapse:collapse }}
th,td {{ text-align:left; padding:.5rem; border-bottom:1px solid #8885 }} th button {{ all:unset;cursor:pointer;font-weight:700 }}
code {{ overflow-wrap:anywhere }} details {{ margin:1rem 0 }} .muted {{ opacity:.7 }} .sr {{ position:absolute;left:-10000px }}
.views button[aria-pressed="true"] {{ font-weight:700;text-decoration:underline }} tr:target {{ outline:2px solid currentColor }}
</style></head><body><h1>Git Slop local report</h1><p id="descriptor" class="muted"></p>
<p id="truncation" role="status"></p>
<nav class="views" aria-label="Report view"><button data-view="files" aria-pressed="true">Files</button> <button data-view="folders" aria-pressed="false">Folders</button> <button data-view="queue" aria-pressed="false">Action queue</button> <button data-view="health" aria-pressed="false">Health findings</button> <button data-view="relationships" aria-pressed="false">Relationships</button> <button data-view="clusters" aria-pressed="false">Clusters</button></nav>
<label for="query" class="sr">Search paths</label><input id="query" type="search" placeholder="Search paths"><label for="profile" class="sr">Profile</label><select id="profile"><option value="">All profiles</option></select>
<label id="severity-label" for="severity" class="sr">Maintenance band</label><select id="severity"><option value="">All maintenance bands</option><option>critical</option><option>high</option><option>moderate</option><option>low</option><option>error</option><option>warning</option><option>notice</option></select>
<p id="sort-state" class="muted" aria-live="polite"></p><p id="count" aria-live="polite"></p><button id="previous" type="button">Previous</button><button id="next" type="button">Next</button><table><caption class="sr">Git Slop records</caption><thead><tr id="headers"></tr></thead><tbody id="rows"></tbody></table>
<details id="file-detail"><summary>Selected record details</summary><pre id="detail"></pre></details>
<details><summary>Evidence summary</summary><pre id="evidence-summary"></pre></details>
<script id="report" type="application/json">{payload}</script><script nonce="{csp_nonce}">
const report=JSON.parse(document.getElementById('report').textContent), params=new URLSearchParams(location.search); let view=params.get('view')||'files', sortKey=params.get('sort')||'slop_score', ascending=params.get('dir')==='asc', page=Number(params.get('page')||0); const pageSize=100;
const files=report.files??[], folders=report.folders??[], queue=report.action_queue??[], findings=report.health?.findings??[], relationships=report.organization?.relationships??[], clusters=report.organization?.clusters??[]; const esc=v=>String(v??'').replace(/[&<>"']/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c]));
document.getElementById('descriptor').textContent=`${{report.repo?.repo_name||'repository'}} · ${{report.generated_at||'unknown time'}} · schema ${{report.schema_version}}`;
const profile=document.getElementById('profile'); [...new Set(files.map(f=>f.profile).filter(Boolean))].sort().forEach(v=>profile.insertAdjacentHTML('beforeend',`<option>${{esc(v)}}</option>`));
document.getElementById('query').value=params.get('q')||''; profile.value=params.get('profile')||''; document.getElementById('severity').value=params.get('band')||'';
const columns={{files:[['path','Path'],['profile','Profile'],['language','Language'],['slop_band','Maintenance'],['context_band','Context'],['slop_score','Score'],['tokens','Tokens']],folders:[['path','Folder'],['classification','Classification'],['health_band','Health'],['context_band','Context'],['slop_score','Score'],['tokens','Tokens']],queue:[['path','Path'],['severity','Severity'],['reason_codes','Reasons'],['evidence_status','Evidence'],['next_action','Next action']],health:[['path','Path'],['severity','Severity'],['title','Finding'],['message','Message']],relationships:[['id','Relationship'],['kind','Kind'],['source_path','Source'],['target_path','Target'],['evidence_score','Evidence']],clusters:[['id','Cluster'],['kind','Kind'],['member_count','Members'],['evidence_score','Evidence']]}};
function records() {{ return view==='folders'?folders:view==='queue'?queue:view==='health'?findings:view==='relationships'?relationships:view==='clusters'?clusters:files }}
function syncUrl() {{ const p=new URLSearchParams(); for (const [k,v] of Object.entries({{view,q:document.getElementById('query').value,profile:profile.value,band:document.getElementById('severity').value,sort:sortKey,dir:ascending?'asc':'desc',page}})) if(v!==''&&v!==0)p.set(k,v); history.replaceState(null,'',`${{location.pathname}}?${{p}}${{location.hash}}`) }}
function render() {{ const q=document.getElementById('query').value.toLowerCase(), p=profile.value, s=document.getElementById('severity').value, source=records();
 document.querySelectorAll('[data-view]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.view===view)));
 const activeColumns=columns[view]??columns.files; if(!activeColumns.some(([key])=>key===sortKey))sortKey=activeColumns[0][0]; document.getElementById('headers').innerHTML=activeColumns.map(([key,label])=>`<th scope="col"><button data-key="${{esc(key)}}" aria-sort="${{key===sortKey?(ascending?'ascending':'descending'):'none'}}">${{esc(label)}}</button></th>`).join('');
 document.getElementById('severity-label').textContent=view==='health'||view==='queue'?'Finding severity':'Maintenance band';
 const profileApplies=view==='files'||view==='queue', bandApplies=view==='files'||view==='folders'||view==='queue'||view==='health'; profile.disabled=!profileApplies; document.getElementById('severity').disabled=!bandApplies;
 const haystack=f=>[f.path,f.id,f.source_path,f.target_path,...(f.members??[])].join(' ').toLowerCase(); const selected=source.filter(f=>(!q||haystack(f).includes(q))&&(!profileApplies||!p||f.profile===p)&&(!bandApplies||!s||(f.slop_band??f.severity)===s)).sort((a,b)=>{{const x=a[sortKey],y=b[sortKey]; return (typeof x==='number'?x-y:String(x??'').localeCompare(String(y??'')))*(ascending?1:-1)}});
 const pages=Math.max(1,Math.ceil(selected.length/pageSize)); page=Math.min(page,pages-1); const visible=selected.slice(page*pageSize,(page+1)*pageSize);
 document.getElementById('count').textContent=`${{selected.length}} of ${{source.length}} ${{view.replace('_',' ')}} records · page ${{page+1}} of ${{pages}}`;
 document.getElementById('previous').disabled=page===0; document.getElementById('next').disabled=page+1>=pages;
 document.getElementById('sort-state').textContent=`Sorted by ${{activeColumns.find(([key])=>key===sortKey)?.[1]??sortKey}}, ${{ascending?'ascending':'descending'}}`;
 document.getElementById('rows').innerHTML=visible.map((f,i)=>`<tr tabindex="0" id="record-${{page*pageSize+i}}" data-index="${{page*pageSize+i}}">${{activeColumns.map(([key],column)=>`<td>${{column===0?`<button class="record"><code>${{esc(f[key]??f.path??f.id)}}</code></button>`:esc(Array.isArray(f[key])?f[key].join(', '):(f[key]??(key==='member_count'?(f.members??[]).length:'')))}}</td>`).join('')}}</tr>`).join('');
 document.querySelectorAll('.record').forEach((button,i)=>button.addEventListener('click',()=>{{document.getElementById('detail').textContent=JSON.stringify(visible[i],null,2);document.getElementById('file-detail').open=true;location.hash=`record-${{page*pageSize+i}}`}})); syncUrl(); }}
document.querySelectorAll('input,select').forEach(el=>el.addEventListener('input',()=>{{page=0;render()}})); document.getElementById('headers').addEventListener('click',event=>{{const el=event.target.closest('button');if(!el)return;ascending=sortKey===el.dataset.key?!ascending:true;sortKey=el.dataset.key;page=0;render()}});
document.querySelectorAll('[data-view]').forEach(el=>el.addEventListener('click',()=>{{view=el.dataset.view;page=0;render()}})); document.getElementById('previous').addEventListener('click',()=>{{page=Math.max(0,page-1);render()}}); document.getElementById('next').addEventListener('click',()=>{{page+=1;render()}});
document.addEventListener('keydown',event=>{{const rows=[...document.querySelectorAll('tbody tr')];const index=rows.indexOf(document.activeElement);if(event.key==='ArrowDown'&&index>=0){{event.preventDefault();rows[Math.min(rows.length-1,index+1)]?.focus()}}if(event.key==='ArrowUp'&&index>=0){{event.preventDefault();rows[Math.max(0,index-1)]?.focus()}}}}); const truncated=Object.entries(report.embedded_evidence?.truncated_views??{{}}).filter(([,value])=>value).map(([key])=>key); document.getElementById('truncation').textContent=truncated.length?`Embedded view limit reached for: ${{truncated.join(', ')}}. Open ${{report.source_report}} for complete evidence.`:''; document.getElementById('evidence-summary').textContent=JSON.stringify({{config_digests:report.config_digests,completeness:report.evidence_completeness,collections:report.collection_metadata,embedded:report.embedded_evidence,source_report:report.source_report}},null,2); render();
</script></body></html>"#
    );
    fs::write(&output, html)?;
    println!("Wrote local HTML report to {}.", output.display());
    Ok(0)
}
