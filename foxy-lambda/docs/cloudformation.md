# CloudFormation Scripts

## Overview
This project uses AWS CloudFormation to manage infrastructure as code. The following scripts are included:

### 1. `create_service_accounts.yaml`
- **Purpose**: Creates an IAM user with access to Cognito and attaches access keys.
- **Resources**:
    - CognitoAccessPolicy: IAM Managed Policy for Cognito actions.
    - CognitoServiceAccount: IAM User with the policy attached.
    - CognitoServiceAccountAccessKey: Access keys for the service account.

### 2. `dev_identity_pool.yaml`
- **Purpose**: Creates an Identity Pool for the application.
- **Resources**:
    - CognitoIdentityPool: Enables unauthenticated identities.
    - Roles for authenticated users.

## Deployment
Use the `cleanup_and_deploy.sh` script to manage the lifecycle of these resources.
