# Authorization Process Documentation

## Overview

The authorization process for this project integrates AWS Cognito User Pools and Identity Pools to securely manage user authentication and authorization. This approach leverages Cognito’s ability to handle user credentials, token generation, and access policies while providing scalability and compliance with best practices.

## Cognito User Pools

Cognito User Pools serve as the primary identity provider for the system. They manage user registration, login, and user profiles.

### Configuration Details

- **User Attributes**: The following attributes are defined in the User Pool schema:
    - `sub`: A unique identifier for each user (required, immutable).
    - `name`: The user's name (required, mutable).
    - `email`: The user's email address (required, mutable, auto-verified).
    - `phone_number`: The user's phone number (optional, mutable).
    - `wallet_address`: A 42-character Ethereum wallet address (optional, mutable).

- **Password Policies**: Passwords must meet the following requirements:
    - Minimum length: 8 characters
    - Include uppercase and lowercase letters
    - Include numbers and symbols

- **Token Validity**: Access tokens, ID tokens, and refresh tokens have customizable validity periods. For this project:
    - Access Token: 1439 minutes (approximately 24 hours)
    - ID Token: 1439 minutes (approximately 24 hours)
    - Refresh Token: 30 days

- **MFA**: Multi-Factor Authentication (MFA) is disabled in this configuration for simplicity during development.

## Cognito Identity Pools

Identity Pools bridge the gap between authenticated Cognito users and AWS resources. Users authenticated via the User Pool are granted temporary AWS credentials via Identity Pools.

### Configuration Details

- **AllowUnauthenticatedIdentities**: Disabled to enforce authentication for all users.
- **Role Attachments**: An IAM role is assigned to authenticated users. This role defines the AWS resources and actions users can access.

## Service Account

A dedicated service account is created to interact programmatically with Cognito. This account is used by the backend to:

- Check if a user exists in the User Pool.
- Create new users with pre-set attributes.
- Set permanent passwords for new users.
- Initiate authentication flows to generate tokens.

### Service Account Setup

The service account is configured via an IAM user with an attached policy that allows:

- `cognito-idp:AdminCreateUser`
- `cognito-idp:AdminInitiateAuth`
- `cognito-idp:ListUsers`
- `cognito-idp:AdminSetUserPassword`

The policy is scoped to the specific User Pool ARN to adhere to the principle of least privilege.

## Technology Choices

1. **AWS Cognito**:
    - Chosen for its seamless integration with AWS services and scalability.
    - Provides out-of-the-box features like token generation, secure user data storage, and user verification.

2. **IAM Policies and Roles**:
    - Enables fine-grained access control.
    - Ensures that only the service account can perform administrative actions on Cognito.

3. **Service Account**:
    - Simplifies backend interactions with Cognito while isolating access.
    - Ensures programmatic operations do not require user-level credentials.

4. **CloudFormation Templates**:
    - Infrastructure as Code (IaC) ensures repeatability and consistency.
    - Facilitates easy updates and rollback of configurations.

## Security Considerations

- **Environment Variables**: The service account's access keys are stored securely in environment variables and passed to the application.
- **Token Validation**: Google ID tokens are validated against the specified client ID to ensure authenticity before interacting with Cognito.
- **IAM Role for CI/CD**: A dedicated IAM role is used for deploying updates, restricting CI/CD pipelines to authorized actions only.

## Future Enhancements

- **Enable MFA**: For production environments, enabling Multi-Factor Authentication (MFA) will enhance security.
- **Fine-Grained Policies**: Define resource-level permissions for more specific access control.
- **Audit Logging**: Enable CloudTrail for monitoring and auditing all actions performed by the service account and users.

## Summary

This implementation leverages AWS Cognito’s powerful authentication and authorization capabilities to provide a secure, scalable foundation for user management. The use of a dedicated service account and fine-grained IAM policies ensures operational efficiency while adhering to security best practices.
