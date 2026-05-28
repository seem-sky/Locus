use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewOutcome {
    Pass,
    NeedsRevision,
    Fail,
}

impl std::fmt::Display for ReviewOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewOutcome::Pass => write!(f, "pass"),
            ReviewOutcome::NeedsRevision => write!(f, "needs_revision"),
            ReviewOutcome::Fail => write!(f, "fail"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub description: String,
    pub location: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueCategory {
    Quality,
    Security,
    Performance,
    Logic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSection {
    pub score: u8,
    pub issues: Vec<ReviewIssue>,
    pub summary: String,
}

impl ReviewSection {
    pub fn new(summary: &str) -> Self {
        Self {
            score: 100,
            issues: Vec::new(),
            summary: summary.to_string(),
        }
    }

    pub fn with_score(mut self, score: u8) -> Self {
        self.score = score.min(100);
        self
    }

    pub fn add_issue(&mut self, issue: ReviewIssue) {
        let deduction = match issue.severity {
            IssueSeverity::Critical => 25,
            IssueSeverity::High => 15,
            IssueSeverity::Medium => 8,
            IssueSeverity::Low => 3,
            IssueSeverity::Info => 1,
        };
        self.score = self.score.saturating_sub(deduction);
        self.issues.push(issue);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    pub quality: ReviewSection,
    pub security: ReviewSection,
    pub performance: ReviewSection,
    pub logic: ReviewSection,
    pub overall: ReviewOutcome,
    pub suggestions: Vec<String>,
}

impl ReviewReport {
    pub fn new() -> Self {
        Self {
            quality: ReviewSection::new("Code quality review"),
            security: ReviewSection::new("Security review"),
            performance: ReviewSection::new("Performance review"),
            logic: ReviewSection::new("Logic correctness review"),
            overall: ReviewOutcome::Pass,
            suggestions: Vec::new(),
        }
    }

    pub fn calculate_overall(&mut self) {
        let avg_score = {
            let total: u32 = self.quality.score as u32
                + self.security.score as u32
                + self.performance.score as u32
                + self.logic.score as u32;
            (total / 4) as u8
        };

        let critical_issues = self
            .quality
            .issues
            .iter()
            .chain(&self.security.issues)
            .chain(&self.performance.issues)
            .chain(&self.logic.issues)
            .filter(|i| i.severity == IssueSeverity::Critical)
            .count();

        let high_issues = self
            .quality
            .issues
            .iter()
            .chain(&self.security.issues)
            .chain(&self.performance.issues)
            .chain(&self.logic.issues)
            .filter(|i| i.severity == IssueSeverity::High)
            .count();

        self.overall = if critical_issues > 0 || avg_score < 40 {
            ReviewOutcome::Fail
        } else if high_issues > 2 || avg_score < 70 {
            ReviewOutcome::NeedsRevision
        } else {
            ReviewOutcome::Pass
        };
    }

    pub fn add_suggestion(&mut self, suggestion: String) {
        self.suggestions.push(suggestion);
    }

    pub fn merge_from(&mut self, other: ReviewReport) {
        self.quality.score = self.quality.score.min(other.quality.score);
        self.quality.issues.extend(other.quality.issues);

        self.security.score = self.security.score.min(other.security.score);
        self.security.issues.extend(other.security.issues);

        self.performance.score = self.performance.score.min(other.performance.score);
        self.performance.issues.extend(other.performance.issues);

        self.logic.score = self.logic.score.min(other.logic.score);
        self.logic.issues.extend(other.logic.issues);

        self.suggestions.extend(other.suggestions);
    }
}

impl Default for ReviewReport {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ReviewReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Code Review Report ===")?;
        writeln!(f, "Quality: {} ({} issues)", self.quality.score, self.quality.issues.len())?;
        writeln!(f, "Security: {} ({} issues)", self.security.score, self.security.issues.len())?;
        writeln!(f, "Performance: {} ({} issues)", self.performance.score, self.performance.issues.len())?;
        writeln!(f, "Logic: {} ({} issues)", self.logic.score, self.logic.issues.len())?;
        writeln!(f, "Overall: {}", self.overall)?;
        if !self.suggestions.is_empty() {
            writeln!(f, "\nSuggestions:")?;
            for (i, s) in self.suggestions.iter().enumerate() {
                writeln!(f, "  {}. {}", i + 1, s)?;
            }
        }
        Ok(())
    }
}
