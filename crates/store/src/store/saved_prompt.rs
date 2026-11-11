use kg_domain::{SavedPrompt, SavedPromptId};
use rusqlite::{OptionalExtension as _, named_params};

use super::found;
use crate::Result;
use crate::column::Text;

impl super::Store {
    pub fn insert_saved_prompt(&self, prompt: &SavedPrompt) -> Result<()> {
        self.conn.execute(
            "INSERT INTO saved_prompts (id, title, body, starred)
             VALUES (:id, :title, :body, :starred)",
            named_params! {
                ":id": Text(&prompt.id),
                ":title": &prompt.title,
                ":body": &prompt.body,
                ":starred": i64::from(prompt.starred),
            },
        )?;
        Ok(())
    }

    pub fn saved_prompt_titled(&self, title: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM saved_prompts WHERE title = :title COLLATE NOCASE)",
                named_params! { ":title": title },
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn saved_prompt(&self, id: &SavedPromptId) -> Result<Option<SavedPrompt>> {
        self.conn
            .query_row(
                "SELECT id, title, body, starred FROM saved_prompts WHERE id = :id",
                named_params! { ":id": Text(id) },
                read_saved_prompt,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn search_saved_prompts(&self, text: &str) -> Result<Vec<SavedPrompt>> {
        let pattern = like_pattern(text);
        let mut statement = self.conn.prepare(
            "SELECT id, title, body, starred FROM saved_prompts
             WHERE lower(title) LIKE lower(:pattern) ESCAPE '\\'
                OR lower(body) LIKE lower(:pattern) ESCAPE '\\'
             ORDER BY starred DESC, title COLLATE NOCASE, id",
        )?;
        let prompts = statement
            .query_map(named_params! { ":pattern": pattern }, read_saved_prompt)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(prompts)
    }

    pub fn update_saved_prompt(&self, id: &SavedPromptId, title: &str, body: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE saved_prompts SET title = :title, body = :body WHERE id = :id",
            named_params! {
                ":id": Text(id),
                ":title": title,
                ":body": body,
            },
        )?;
        found(changed, "saved prompt", id)
    }

    pub fn set_saved_prompt_starred(&self, id: &SavedPromptId, starred: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE saved_prompts SET starred = :starred WHERE id = :id",
            named_params! {
                ":id": Text(id),
                ":starred": i64::from(starred),
            },
        )?;
        found(changed, "saved prompt", id)
    }

    pub fn delete_saved_prompt(&self, id: &SavedPromptId) -> Result<()> {
        let changed = self.conn.execute(
            "DELETE FROM saved_prompts WHERE id = :id",
            named_params! { ":id": Text(id) },
        )?;
        found(changed, "saved prompt", id)
    }
}

fn read_saved_prompt(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedPrompt> {
    Ok(SavedPrompt {
        id: row.get::<_, Text<_>>("id")?.0,
        title: row.get("title")?,
        body: row.get("body")?,
        starred: row.get::<_, i64>("starred")? != 0,
    })
}

fn like_pattern(text: &str) -> String {
    let mut pattern = String::from('%');
    for character in text.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}
