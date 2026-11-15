//! Line-oriented control-thread editor, also usable with piped scripts.
use oxide_core::{
    edit::{EditCommand, ProjectEditor},
    Project,
};
use std::{
    io::{self, BufRead},
    path::Path,
};

pub fn run(path: &Path) -> crate::Result<()> {
    let mut editor = ProjectEditor::new(Project::load(path)?)?;
    println!(
        "Editing {}. Enter a JSON command or array (one atomic batch per line).",
        path.display()
    );
    println!("Actions: undo, redo, show, save, quit. Only save writes the project; EOF quits.");
    let mut rejected = false;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let line = line.trim();
        let result: crate::Result<()> = (|| {
            match line {
                "" | "quit" => {}
                "undo" => println!(
                    "{}",
                    if editor.undo() {
                        "Undone"
                    } else {
                        "Nothing to undo"
                    }
                ),
                "redo" => println!(
                    "{}",
                    if editor.redo() {
                        "Redone"
                    } else {
                        "Nothing to redo"
                    }
                ),
                "show" => print!("{}", editor.project().to_json()?),
                "save" => {
                    editor.project().save(path)?;
                    println!("Saved {}", path.display());
                }
                _ => {
                    let commands = if line.starts_with('[') {
                        serde_json::from_str::<Vec<EditCommand>>(line)?
                    } else {
                        vec![serde_json::from_str::<EditCommand>(line)?]
                    };
                    let changed = editor.apply_batch(&commands)?;
                    println!("{}", if changed { "Applied" } else { "Unchanged" });
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Edit rejected: {error}");
            rejected = true;
        }
        if line == "quit" {
            break;
        }
    }
    if rejected {
        return Err("One or more editor actions failed".into());
    }
    Ok(())
}
