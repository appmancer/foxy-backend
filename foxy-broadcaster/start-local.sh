#!/bin/bash

# Fetch the exported role name dynamically
ROLE_EXPORT_NAME="Foxy-dev-FoxyLambdaSQSRoleName"
ROLE_NAME=$(aws cloudformation list-exports \
    --query "Exports[?Name=='${ROLE_EXPORT_NAME}'].Value" \
    --output text)

# Construct the role ARN dynamically
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query "Account" --output text)
ROLE_ARN="arn:aws:iam::${AWS_ACCOUNT_ID}:role/${ROLE_NAME}"

echo "Using Role ARN: $ROLE_ARN"

# Configuration
SESSION_NAME="DevSession"

# Assume the role and get credentials
CREDS=$(aws sts assume-role --role-arn "$ROLE_ARN" --role-session-name "$SESSION_NAME" --query 'Credentials' --output json)

if [ $? -ne 0 ]; then
  echo "Failed to assume role. Check your role ARN and permissions."
  exit 1
fi

# Export credentials to environment variables
export AWS_ACCESS_KEY_ID=$(echo "$CREDS" | jq -r '.AccessKeyId')
export AWS_SECRET_ACCESS_KEY=$(echo "$CREDS" | jq -r '.SecretAccessKey')
export AWS_SESSION_TOKEN=$(echo "$CREDS" | jq -r '.SessionToken')

echo "Temporary AWS credentials set for role: $ROLE_ARN"

RUST_LOG=error,foxy_broadcaster=info RUST_BACKTRACE=1 cargo lambda watch --invoke-port 9001 --verbose
