use crate::agent::review::{
    IssueCategory, IssueSeverity, ReviewIssue, ReviewReport, ReviewSection,
};
use crate::agent::task::CodeChanges;
use std::path::Path;

pub struct CodeReviewer {
    project_root: String,
}

impl CodeReviewer {
    pub fn new(project_root: String) -> Self {
        Self { project_root }
    }

    pub fn review_changes(&self, changes: &CodeChanges) -> ReviewReport {
        let mut report = ReviewReport::new();

        for file_path in &changes.files_created {
            self.review_file(file_path, true, &mut report);
        }

        for modification in &changes.files_modified {
            self.review_modified_file(&modification.path, &modification.new_content, &mut report);
        }

        report.calculate_overall();
        report
    }

    fn review_file(&self, file_path: &str, is_new: bool, report: &mut ReviewReport) {
        let content = if let Ok(c) = std::fs::read_to_string(file_path) {
            c
        } else {
            return;
        };

        self.check_quality(file_path, &content, report);
        self.check_security(file_path, &content, report);
        self.check_performance(file_path, &content, report);
        self.check_logic(file_path, &content, report);
    }

    fn review_modified_file(&self, file_path: &str, new_content: &str, report: &mut ReviewReport) {
        self.check_quality(file_path, new_content, report);
        self.check_security(file_path, new_content, report);
        self.check_performance(file_path, new_content, report);
        self.check_logic(file_path, new_content, report);
    }

    fn check_quality(&self, file_path: &str, content: &str, report: &mut ReviewReport) {
        let mut section = ReviewSection::new("Code quality review");
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let line_count = content.lines().count();
        if line_count > 500 {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Medium,
                category: IssueCategory::Quality,
                description: format!("File has {} lines, consider splitting large files (>500 lines)", line_count),
                location: Some(file_path.to_string()),
                suggestion: Some("Consider extracting related logic into separate modules or files".to_string()),
            });
        }

        if content.lines().any(|l| l.len() > 150) {
            let long_lines: usize = content.lines().filter(|l| l.len() > 150).count();
            if long_lines > 5 {
                section.add_issue(ReviewIssue {
                    severity: IssueSeverity::Low,
                    category: IssueCategory::Quality,
                    description: format!("File has {} lines exceeding 150 characters", long_lines),
                    location: Some(file_path.to_string()),
                    suggestion: Some("Break long lines for better readability".to_string()),
                });
            }
        }

        let suspicious_patterns = [
            ("TODO", "Found TODO comment - should be addressed before shipping"),
            ("FIXME", "Found FIXME comment - indicates known issue needing fix"),
            ("XXX", "Found XXX comment - indicates problematic code"),
            ("HACK", "Found HACK comment - indicates workaround code"),
        ];

        for (pattern, desc) in &suspicious_patterns {
            if content.contains(pattern) {
                section.add_issue(ReviewIssue {
                    severity: IssueSeverity::Info,
                    category: IssueCategory::Quality,
                    description: desc.to_string(),
                    location: Some(file_path.to_string()),
                    suggestion: Some(format!("Address or create tracking issue for {}", pattern)),
                });
            }
        }

        match ext {
            "rs" => self.check_rust_quality(file_path, content, &mut section),
            "ts" | "tsx" | "js" | "jsx" | "vue" => self.check_js_quality(file_path, content, &mut section),
            _ => {}
        }

        report.quality.issues.extend(section.issues);
        if section.score < report.quality.score {
            report.quality.score = section.score;
        }
    }

    fn check_rust_quality(&self, file_path: &str, content: &str, section: &mut ReviewSection) {
        if content.contains(".unwrap()") {
            let unwrap_count = content.matches(".unwrap()").count();
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Medium,
                category: IssueCategory::Quality,
                description: format!("Found {} .unwrap() calls - consider proper error handling", unwrap_count),
                location: Some(file_path.to_string()),
                suggestion: Some("Use ? operator or proper error handling with Result/Option".to_string()),
            });
        }

        if content.contains("unsafe ") && !content.contains("unsafe impl") && !content.contains("unsafe fn") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::High,
                category: IssueCategory::Quality,
                description: "unsafe block detected - ensure memory safety invariants are upheld".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Document why unsafe is necessary and what invariants it maintains".to_string()),
            });
        }

        if content.contains("println!") || content.contains("eprintln!") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Quality,
                description: "Debug print statement found - remove before production".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Use proper logging framework instead".to_string()),
            });
        }
    }

    fn check_js_quality(&self, file_path: &str, content: &str, section: &mut ReviewSection) {
        if content.contains("var ") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Low,
                category: IssueCategory::Quality,
                description: "Found 'var' declaration - use 'const' or 'let' instead".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Modern JavaScript uses const/let for better scoping".to_string()),
            });
        }

        if content.contains("== ") || content.contains(" ==") || content.contains("!=") || content.contains(" =!") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Medium,
                category: IssueCategory::Quality,
                description: "Found loose equality operators (==/!=) - use strict equality (===/!==)".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Strict equality prevents type coercion bugs".to_string()),
            });
        }

        if content.contains("any") && file_path.ends_with(".ts") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Low,
                category: IssueCategory::Quality,
                description: "Found 'any' type - avoid any for type safety".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Use specific types or unknown with type guards".to_string()),
            });
        }
    }

    fn check_security(&self, file_path: &str, content: &str, report: &mut ReviewReport) {
        let mut section = ReviewSection::new("Security review");
        let lower = content.to_lowercase();

        let dangerous_patterns = [
            ("eval(", "Dynamic code execution with eval() - potential code injection"),
            ("innerHTML", "Direct innerHTML assignment - potential XSS vulnerability"),
            ("document.write", "document.write usage - potential XSS vulnerability"),
            ("SQL", "SQL query detected - ensure parameterized queries are used"),
            ("password", "Password-related code - ensure secure handling"),
            ("secret", "Secret handling - ensure secrets are not hardcoded"),
            ("api_key", "API key detected - ensure keys are not hardcoded"),
            ("token", "Token handling - ensure secure storage and transmission"),
            ("crypto", "Cryptographic operations - ensure proper crypto library usage"),
            ("hmac", "HMAC operations - ensure key management"),
        ];

        for (pattern, desc) in &dangerous_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                let severity: IssueSeverity = match *pattern {
                    "eval(" | "innerHTML" | "document.write" => IssueSeverity::Critical,
                    "password" | "secret" | "api_key" | "token" | "crypto" => IssueSeverity::High,
                    _ => IssueSeverity::Medium,
                };
                section.add_issue(ReviewIssue {
                    severity,
                    category: IssueCategory::Security,
                    description: desc.to_string(),
                    location: Some(file_path.to_string()),
                    suggestion: Some(format!("Review {} usage for security implications", pattern)),
                });
            }
        }

        if content.contains("localhost") && content.contains("api") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Security,
                description: "Development URL detected - ensure production uses proper endpoints".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Use environment variables for API endpoints".to_string()),
            });
        }

        report.security.issues.extend(section.issues);
        if section.score < report.security.score {
            report.security.score = section.score;
        }
    }

    fn check_performance(&self, file_path: &str, content: &str, report: &mut ReviewReport) {
        let mut section = ReviewSection::new("Performance review");
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if content.contains("for (") || content.contains("while (") {
            if content.contains("O(n²)") || content.contains("O(n^2)") {
                section.add_issue(ReviewIssue {
                    severity: IssueSeverity::High,
                    category: IssueCategory::Performance,
                    description: "Potential quadratic complexity detected".to_string(),
                    location: Some(file_path.to_string()),
                    suggestion: Some("Consider optimizing nested loops".to_string()),
                });
            }
        }

        if content.contains("JSON.parse") && content.contains("JSON.stringify") {
            let parse_count = content.matches("JSON.parse").count();
            let stringify_count = content.matches("JSON.stringify").count();
            if parse_count > 10 || stringify_count > 10 {
                section.add_issue(ReviewIssue {
                    severity: IssueSeverity::Low,
                    category: IssueCategory::Performance,
                    description: format!("Heavy JSON serialization/deserialization ({} parse, {} stringify)", parse_count, stringify_count),
                    location: Some(file_path.to_string()),
                    suggestion: Some("Consider caching or reducing serialization frequency".to_string()),
                });
            }
        }

        match ext {
            "rs" => self.check_rust_performance(file_path, content, &mut section),
            "ts" | "tsx" | "js" | "jsx" => self.check_js_performance(file_path, content, &mut section),
            _ => {}
        }

        report.performance.issues.extend(section.issues);
        if section.score < report.performance.score {
            report.performance.score = section.score;
        }
    }

    fn check_rust_performance(&self, file_path: &str, content: &str, section: &mut ReviewSection) {
        if content.contains(".clone()") {
            let clone_count = content.matches(".clone()").count();
            if clone_count > 10 {
                section.add_issue(ReviewIssue {
                    severity: IssueSeverity::Low,
                    category: IssueCategory::Performance,
                    description: format!("Found {} .clone() calls - consider using references or Rc/Arc", clone_count),
                    location: Some(file_path.to_string()),
                    suggestion: Some("Excessive cloning impacts memory and performance".to_string()),
                });
            }
        }

        if content.contains("String::from(") && content.contains("&str") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Performance,
                description: "String conversion detected - consider using &str directly".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Use &str instead of String::from() where possible".to_string()),
            });
        }

        if content.contains("collect::<Vec") && content.contains("iter()") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Performance,
                description: "Iterator collection pattern - ensure it's necessary".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Consider if iteration can be combined with the operation".to_string()),
            });
        }
    }

    fn check_js_performance(&self, file_path: &str, content: &str, section: &mut ReviewSection) {
        if content.contains("useEffect") || content.contains("componentDidUpdate") {
            if content.contains("setState") && !content.contains("useMemo") && !content.contains("useCallback") {
                section.add_issue(ReviewIssue {
                    severity: IssueSeverity::Low,
                    category: IssueCategory::Performance,
                    description: "State update in lifecycle - consider memoization".to_string(),
                    location: Some(file_path.to_string()),
                    suggestion: Some("Wrap callbacks in useCallback and values in useMemo".to_string()),
                });
            }
        }

        if content.contains("Object.assign") && content.contains("{}") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Performance,
                description: "Object.assign with empty object detected".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Consider spread operator or Object.assign without empty object".to_string()),
            });
        }

        if content.matches("async").count() > 5 && content.contains("await") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Performance,
                description: "Multiple async operations - consider Promise.all for parallel execution".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Use Promise.all() to run independent async operations in parallel".to_string()),
            });
        }
    }

    fn check_logic(&self, file_path: &str, content: &str, report: &mut ReviewReport) {
        let mut section = ReviewSection::new("Logic correctness review");

        if content.contains("if (true)") || content.contains("if (false)") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Critical,
                category: IssueCategory::Logic,
                description: "Constant conditional expression detected".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Remove dead code or fix the condition".to_string()),
            });
        }

        let loops_with_no_break = content.lines()
            .filter(|l| l.contains("while") || l.contains("for"))
            .filter(|l| !l.contains("break") && !l.contains("return") && !l.contains("continue"))
            .count();

        if loops_with_no_break > 3 {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Medium,
                category: IssueCategory::Logic,
                description: format!("Multiple loops without break/return detected ({})", loops_with_no_break),
                location: Some(file_path.to_string()),
                suggestion: Some("Verify loops have proper exit conditions".to_string()),
            });
        }

        if content.contains("catch") && !content.contains("error") && !content.contains("err") {
            section.add_issue(ReviewIssue {
                severity: IssueSeverity::Low,
                category: IssueCategory::Logic,
                description: "Empty or ignored exception caught".to_string(),
                location: Some(file_path.to_string()),
                suggestion: Some("Handle or log exceptions properly".to_string()),
            });
        }

        report.logic.issues.extend(section.issues);
        if section.score < report.logic.score {
            report.logic.score = section.score;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_new_file() {
        let reviewer = CodeReviewer::new(".".to_string());
        let mut changes = CodeChanges::default();
        changes.files_created.push("test.rs".to_string());

        let report = reviewer.review_changes(&changes);
        assert!(report.quality.score <= 100);
    }

    #[test]
    fn test_security_detection() {
        let reviewer = CodeReviewer::new(".".to_string());
        let mut changes = CodeChanges::default();
        changes.files_created.push("test.js".to_string());

        std::fs::write("test.js", "eval(userInput)").unwrap();
        let report = reviewer.review_changes(&changes);

        assert!(report.security.issues.iter().any(|i| i.category == IssueCategory::Security));
        std::fs::remove_file("test.js").unwrap();
    }
}
