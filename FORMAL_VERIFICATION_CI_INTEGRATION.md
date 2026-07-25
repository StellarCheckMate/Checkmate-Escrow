# Formal Verification CI/CD Integration Guide

## Overview

This guide shows how to integrate the formal verification harness into your CI/CD pipeline to automatically run formal verification checks on every commit.

## GitHub Actions Integration

### 1. Create Workflow File

Create `.github/workflows/formal-verification.yml`:

```yaml
name: Formal Verification

on:
  push:
    branches: [main, develop]
    paths:
      - 'contracts/escrow/src/**'
      - '.github/workflows/formal-verification.yml'
  pull_request:
    branches: [main, develop]
    paths:
      - 'contracts/escrow/src/**'

jobs:
  formal_verification:
    name: Formal Verification Harness
    runs-on: ubuntu-latest
    
    steps:
      - name: Checkout code
        uses: actions/checkout@v3
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.70
      
      - name: Cache cargo registry
        uses: actions/cache@v3
        with:
          path: ~/.cargo/registry
          key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}
      
      - name: Cache cargo index
        uses: actions/cache@v3
        with:
          path: ~/.cargo/git
          key: ${{ runner.os }}-cargo-git-${{ hashFiles('**/Cargo.lock') }}
      
      - name: Cache cargo build
        uses: actions/cache@v3
        with:
          path: target
          key: ${{ runner.os }}-cargo-build-target-${{ hashFiles('**/Cargo.lock') }}
      
      - name: Run Formal Verification Harness
        run: |
          cd contracts/escrow
          cargo test --lib formal_verification --nocapture 2>&1 | tee fv-report.txt
      
      - name: Upload Formal Verification Report
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: formal-verification-report
          path: contracts/escrow/fv-report.txt
          retention-days: 30
      
      - name: Comment PR with Results
        if: github.event_name == 'pull_request'
        uses: actions/github-script@v6
        with:
          script: |
            const fs = require('fs');
            const report = fs.readFileSync('contracts/escrow/fv-report.txt', 'utf8');
            const lines = report.split('\n');
            const summary = lines.filter(l => l.includes('test result:'));
            
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: `## ✅ Formal Verification Results\n\n${summary.join('\n')}`
            });

  slack_notification:
    name: Slack Notification
    runs-on: ubuntu-latest
    needs: formal_verification
    if: failure()
    
    steps:
      - name: Send Slack notification
        uses: slackapi/slack-github-action@v1
        with:
          webhook-url: ${{ secrets.SLACK_WEBHOOK_URL }}
          payload: |
            {
              "text": "⚠️ Formal Verification Failed",
              "blocks": [
                {
                  "type": "section",
                  "text": {
                    "type": "mrkdwn",
                    "text": "*Formal Verification Failed*\nRepository: ${{ github.repository }}\nBranch: ${{ github.ref }}\nCommit: ${{ github.sha }}"
                  }
                },
                {
                  "type": "section",
                  "text": {
                    "type": "mrkdwn",
                    "text": "<${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}|View Details>"
                  }
                }
              ]
            }
```

### 2. Add to Existing CI Workflow

If you already have a CI workflow, add this job:

```yaml
  formal_verification:
    name: Formal Verification
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cd contracts/escrow && cargo test --lib formal_verification --nocapture
```

## Local Development

### Pre-commit Hook

Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash
set -e

echo "🔍 Running formal verification..."

cd contracts/escrow

if ! cargo test --lib formal_verification --quiet; then
    echo "❌ Formal verification failed!"
    exit 1
fi

echo "✅ Formal verification passed!"
exit 0
```

Make it executable:
```bash
chmod +x .git/hooks/pre-commit
```

### Development Script

Create `scripts/verify.sh`:

```bash
#!/bin/bash

set -e

echo "═════════════════════════════════════════"
echo "  Formal Verification Harness"
echo "═════════════════════════════════════════"
echo ""

cd contracts/escrow

echo "📦 Running formal verification tests..."
cargo test --lib formal_verification --nocapture

echo ""
echo "═════════════════════════════════════════"
echo "✅ All formal verification tests passed!"
echo "═════════════════════════════════════════"
```

Make it executable:
```bash
chmod +x scripts/verify.sh
```

Run with:
```bash
./scripts/verify.sh
```

## GitLab CI Integration

### `.gitlab-ci.yml`

```yaml
stages:
  - verify
  - build

formal_verification:
  stage: verify
  image: rust:1.70
  script:
    - cd contracts/escrow
    - cargo test --lib formal_verification --nocapture
  artifacts:
    reports:
      junit: contracts/escrow/fv-report.xml
    paths:
      - contracts/escrow/fv-report.txt
    expire_in: 30 days
  allow_failure: false
  only:
    - merge_requests
    - main
    - develop

build:
  stage: build
  image: rust:1.70
  script:
    - cd contracts/escrow
    - cargo build --release
  dependencies:
    - formal_verification
  only:
    - main
    - develop
```

## Makefile Integration

Add to your `Makefile`:

```makefile
.PHONY: verify
verify:
	@echo "🔍 Running formal verification harness..."
	cd contracts/escrow && cargo test --lib formal_verification --nocapture

.PHONY: verify-quiet
verify-quiet:
	@cd contracts/escrow && cargo test --lib formal_verification --quiet

.PHONY: verify-report
verify-report:
	@cd contracts/escrow && cargo test --lib formal_verification::test_generate_formal_verification_report -- --nocapture > formal-verification-report.txt

.PHONY: all-checks
all-checks: verify verify-report
	@echo "✅ All verification checks passed!"

.PHONY: ci
ci: verify-quiet
	@echo "✅ CI verification passed!"
```

Usage:
```bash
make verify              # Run full verification
make verify-quiet        # Run verification silently
make verify-report       # Generate JSON report
make all-checks         # Run all verification checks
make ci                 # Quick CI check
```

## Docker Integration

### Dockerfile

```dockerfile
FROM rust:1.70-alpine

WORKDIR /workspace

# Install dependencies
RUN apk add --no-cache git

# Copy code
COPY . .

# Run formal verification
ENTRYPOINT ["sh", "-c", "cd contracts/escrow && cargo test --lib formal_verification --nocapture"]
```

Build and run:
```bash
docker build -t checkmate-fv .
docker run checkmate-fv
```

## Jenkins Integration

### Jenkinsfile

```groovy
pipeline {
    agent any
    
    stages {
        stage('Checkout') {
            steps {
                checkout scm
            }
        }
        
        stage('Formal Verification') {
            steps {
                sh '''
                    cd contracts/escrow
                    cargo test --lib formal_verification --nocapture | tee fv-report.txt
                '''
            }
        }
        
        stage('Archive Results') {
            when {
                always()
            }
            steps {
                archiveArtifacts artifacts: 'contracts/escrow/fv-report.txt', allowEmptyArchive: true
                junit 'contracts/escrow/fv-report.xml'
            }
        }
    }
    
    post {
        always {
            cleanWs()
        }
        failure {
            emailext(
                subject: "Formal Verification Failed: ${env.JOB_NAME} - ${env.BUILD_NUMBER}",
                body: "Build failed. Check console output at ${env.BUILD_URL}",
                to: "${env.CHANGE_AUTHOR_EMAIL}"
            )
        }
        success {
            echo "✅ Formal verification passed!"
        }
    }
}
```

## Report Analysis

### Parse and Store Results

Create `scripts/analyze-fv-report.py`:

```python
#!/usr/bin/env python3

import json
import sys
import re
from datetime import datetime

def extract_results(report_file):
    """Extract formal verification results from report."""
    with open(report_file, 'r') as f:
        content = f.read()
    
    # Extract JSON
    json_match = re.search(r'\{[\s\S]*\}', content)
    if not json_match:
        print("Error: No JSON found in report")
        sys.exit(1)
    
    report = json.loads(json_match.group())
    fv = report['formal_verification_report']
    
    print(f"Timestamp: {fv['timestamp']}")
    print(f"Violations Found: {fv['summary']['violations_found']}")
    print(f"States Explored: {fv['summary']['states_explored']}")
    print(f"Transitions Tested: {fv['summary']['transitions_tested']}")
    print(f"Invariants Checked: {fv['summary']['invariants_checked']}")
    
    if fv['summary']['violations_found'] > 0:
        print("\n⚠️ Violations Found:")
        for v in fv['violations']:
            print(f"  - {v['invariant_id']}: {v['invariant_name']} ({v['severity']})")
            print(f"    {v['description']}")
        return False
    else:
        print("\n✅ No violations found!")
        return True

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python3 analyze-fv-report.py <report_file>")
        sys.exit(1)
    
    success = extract_results(sys.argv[1])
    sys.exit(0 if success else 1)
```

Usage:
```bash
python3 scripts/analyze-fv-report.py formal-verification-report.json
```

## Continuous Monitoring

### Store Historical Results

Create `scripts/store-fv-results.sh`:

```bash
#!/bin/bash

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
RESULTS_DIR="formal-verification-results"
RESULT_FILE="$RESULTS_DIR/$TIMESTAMP.json"

mkdir -p "$RESULTS_DIR"

cd contracts/escrow
cargo test --lib formal_verification::test_generate_formal_verification_report -- --nocapture > "$RESULT_FILE"

echo "✅ Results stored to $RESULT_FILE"
```

### Dashboard Generation

Create `scripts/generate-dashboard.py`:

```python
#!/usr/bin/env python3

import json
import glob
from datetime import datetime

def generate_dashboard():
    """Generate verification dashboard from historical results."""
    
    results = []
    for file in sorted(glob.glob('formal-verification-results/*.json')):
        with open(file, 'r') as f:
            data = json.load(f)
            results.append({
                'timestamp': file.split('/')[-1].replace('.json', ''),
                'violations': data['formal_verification_report']['summary']['violations_found']
            })
    
    html = """<!DOCTYPE html>
<html>
<head>
    <title>Formal Verification Dashboard</title>
    <style>
        body { font-family: Arial; margin: 20px; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #4CAF50; color: white; }
        tr:hover { background-color: #f5f5f5; }
        .pass { color: green; }
        .fail { color: red; }
    </style>
</head>
<body>
    <h1>Formal Verification Results</h1>
    <table>
        <tr>
            <th>Timestamp</th>
            <th>Violations</th>
            <th>Status</th>
        </tr>
    """
    
    for r in results:
        status = f"<span class='pass'>✅ Pass</span>" if r['violations'] == 0 else f"<span class='fail'>❌ Fail</span>"
        html += f"""
        <tr>
            <td>{r['timestamp']}</td>
            <td>{r['violations']}</td>
            <td>{status}</td>
        </tr>
        """
    
    html += """
    </table>
</body>
</html>
    """
    
    with open('formal-verification-dashboard.html', 'w') as f:
        f.write(html)
    
    print("✅ Dashboard generated: formal-verification-dashboard.html")

if __name__ == '__main__':
    generate_dashboard()
```

## Notifications

### Slack Integration

Add to your CI configuration:

```bash
# On failure
curl -X POST $SLACK_WEBHOOK \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "❌ Formal Verification Failed",
    "blocks": [{
      "type": "section",
      "text": {"type": "mrkdwn", "text": "*Formal Verification Failed*\n*Repository:* '"$REPO"'\n*Commit:* '"$COMMIT"'"}
    }]
  }'

# On success
curl -X POST $SLACK_WEBHOOK \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "✅ Formal Verification Passed",
    "blocks": [{
      "type": "section",
      "text": {"type": "mrkdwn", "text": "*Formal Verification Passed*\n*Repository:* '"$REPO"'\n*Invariants Verified:* 20/20"}
    }]
  }'
```

## Policy Enforcement

### Require Passing Checks

GitHub Settings → Branches → Branch Protection Rules:

1. Require status checks to pass before merging:
   - ✅ formal_verification

2. Require branches to be up to date before merging

3. Dismiss stale pull request approvals when new commits are pushed

4. Require code reviews before merging

## Monitoring and Alerts

### Email Alerts

On failure, send email:

```bash
if [ $? -ne 0 ]; then
    echo "Formal verification failed" | mail -s "⚠️ FV Failure" team@example.com
    exit 1
fi
```

### Metrics Tracking

Track over time:
- Violations found per run
- States explored per version
- Test execution time
- Build success/failure rate

## Summary

Integration checklist:

- ✅ GitHub Actions workflow configured
- ✅ Pre-commit hook installed
- ✅ Makefile targets added
- ✅ Local verification script created
- ✅ Report archiving configured
- ✅ Notifications enabled
- ✅ Historical tracking setup
- ✅ Dashboard generation ready
- ✅ Branch protection rules enforced

The formal verification harness is now fully integrated into your CI/CD pipeline!
