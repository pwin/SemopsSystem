// HOLOS on Azure Container Apps, with its data on an Azure Files premium NFS 4.1 share.
//
// Three modes, one template (deploy/azure/README.md says when to pick which):
//
//   snapshot  An in-memory store loaded at start from an N-Quads snapshot on the share.
//             Read-only, scales out and to zero. The published-endpoint option.
//   replica   Read-only replicas that open the newest RocksDB checkpoint on the share with
//             no lock and no writes (holos-server --store-read-only). Scales out; starts
//             in seconds whatever the store's size.
//   writer    One writable store on the share. NFS 4.1 keeps byte-range locks on the
//             server, so a second writer is refused rather than let in, but every
//             synchronous write is a network round trip: small or read-mostly stores. A
//             large or write-heavy store belongs on AKS with a managed disk.
//
// Entra ID sign-in, when `entraClientId` is set, is Container Apps' built-in
// authentication with unauthenticated requests refused; HOLOS then reads the verified
// identity from X-MS-CLIENT-PRINCIPAL (--trust-azure-easy-auth).

targetScope = 'resourceGroup'

@description('Prefix for every resource name; lower case letters and digits.')
@minLength(3)
@maxLength(12)
param namePrefix string

param location string = resourceGroup().location

@description('The SemOps System image. Pin a digest in production.')
param image string = 'ghcr.io/pwin/semops:0.29.0'

@allowed(['snapshot', 'replica', 'writer'])
param mode string = 'replica'

@description('snapshot mode: the N-Quads (optionally gzipped) file on the share to load, relative to the share root.')
param snapshotFile string = 'snapshots/latest.nq.gz'

@minValue(0)
param minReplicas int = mode == 'snapshot' ? 0 : 1

@minValue(1)
param maxReplicas int = mode == 'writer' ? 1 : 4

@description('Entra ID application (client) id for built-in authentication. Empty leaves the endpoint anonymous.')
param entraClientId string = ''

@description('Entra ID tenant id; defaults to this subscription\'s tenant.')
param entraTenantId string = subscription().tenantId

@description('Share size in GiB. Premium Azure Files provisions IOPS and throughput by size.')
@minValue(100)
param shareGiB int = 256

var writer = mode == 'writer'
var authEnabled = !empty(entraClientId)
var storageName = take('${namePrefix}nfs${uniqueString(resourceGroup().id)}', 24)
var mountPath = '/mnt/holos'

// The server arguments every mode shares.
var common = concat(
  [
    '--listen'
    '0.0.0.0:7878'
    '--no-ui'
    '--timeout'
    '300'
    '--max-body'
    '256'
    '--max-query-memory'
    '2'
  ],
  authEnabled ? ['--trust-azure-easy-auth'] : []
)

// replica: the newest finished checkpoint (RocksDB writes `<name>.tmp` and renames it).
var replicaScript = 'set -eu; latest=$(ls -1d ${mountPath}/checkpoints/holos-* 2>/dev/null | grep -v \'\\.tmp$\' | sort | tail -n 1 || true); [ -n "$latest" ] || { echo "no checkpoint under ${mountPath}/checkpoints" >&2; exit 1; }; exec holos-server "$@" --store "$latest" --store-read-only'

var command = mode == 'replica' ? ['/bin/sh', '-c'] : []
var args = mode == 'replica'
  ? concat([replicaScript, 'holos-server'], common)
  : mode == 'snapshot'
      ? concat(common, ['--read-only', '--data', '${mountPath}/${snapshotFile}'])
      : concat(common, ['--store', '${mountPath}/store'])

resource logs 'Microsoft.OperationalInsights/workspaces@2023-09-01' = {
  name: '${namePrefix}-logs'
  location: location
  properties: {
    sku: { name: 'PerGB2018' }
    retentionInDays: 30
  }
}

// NFS shares are reachable only from a virtual network: the Container Apps environment's
// subnet, through a service endpoint, and nothing else.
resource vnet 'Microsoft.Network/virtualNetworks@2024-05-01' = {
  name: '${namePrefix}-vnet'
  location: location
  properties: {
    addressSpace: { addressPrefixes: ['10.40.0.0/16'] }
    subnets: [
      {
        name: 'apps'
        properties: {
          addressPrefix: '10.40.0.0/23'
          delegations: [
            { name: 'apps', properties: { serviceName: 'Microsoft.App/environments' } }
          ]
          serviceEndpoints: [{ service: 'Microsoft.Storage' }]
        }
      }
    ]
  }
}

resource storage 'Microsoft.Storage/storageAccounts@2023-05-01' = {
  name: storageName
  location: location
  kind: 'FileStorage'
  sku: { name: 'Premium_LRS' }
  properties: {
    // NFS 4.1 does not run over the HTTPS transport this setting enforces; the share is
    // instead confined to the virtual network below, with no public access at all.
    supportsHttpsTrafficOnly: false
    minimumTlsVersion: 'TLS1_2'
    allowBlobPublicAccess: false
    // NFS needs no account key; turning keys off removes a credential that could leak.
    allowSharedKeyAccess: false
    publicNetworkAccess: 'Enabled'
    networkAcls: {
      defaultAction: 'Deny'
      bypass: 'None'
      virtualNetworkRules: [
        { id: vnet.properties.subnets[0].id, action: 'Allow' }
      ]
    }
  }
}

resource files 'Microsoft.Storage/storageAccounts/fileServices@2023-05-01' = {
  parent: storage
  name: 'default'
}

resource share 'Microsoft.Storage/storageAccounts/fileServices/shares@2023-05-01' = {
  parent: files
  name: 'holos'
  properties: {
    enabledProtocols: 'NFS'
    rootSquash: 'RootSquash'
    shareQuota: shareGiB
  }
}

resource env 'Microsoft.App/managedEnvironments@2025-01-01' = {
  name: '${namePrefix}-env'
  location: location
  properties: {
    appLogsConfiguration: {
      destination: 'log-analytics'
      logAnalyticsConfiguration: {
        customerId: logs.properties.customerId
        sharedKey: logs.listKeys().primarySharedKey
      }
    }
    vnetConfiguration: {
      infrastructureSubnetId: vnet.properties.subnets[0].id
      internal: false
    }
    workloadProfiles: [{ name: 'Consumption', workloadProfileType: 'Consumption' }]
  }
}

resource envStorage 'Microsoft.App/managedEnvironments/storages@2025-01-01' = {
  parent: env
  name: 'holos'
  properties: {
    nfsAzureFile: {
      server: '${storage.name}.file.${environment().suffixes.storage}'
      shareName: '/${storage.name}/${share.name}'
      accessMode: writer ? 'ReadWrite' : 'ReadOnly'
    }
  }
}

resource app 'Microsoft.App/containerApps@2025-01-01' = {
  name: '${namePrefix}-holos'
  location: location
  properties: {
    environmentId: env.id
    workloadProfileName: 'Consumption'
    configuration: {
      // One revision at a time. For the writer it also matters that the old revision has
      // let go of the store before the new one opens it: the store's lock refuses the
      // newcomer until then, and it is restarted until the lock is free.
      activeRevisionsMode: 'Single'
      ingress: {
        external: true
        targetPort: 7878
        transport: 'auto'
        allowInsecure: false
      }
    }
    template: {
      containers: [
        {
          name: 'holos'
          image: image
          command: command
          args: args
          resources: { cpu: json('1.0'), memory: '2Gi' }
          env: [{ name: 'TMPDIR', value: '/tmp' }]
          volumeMounts: [{ volumeName: 'holos', mountPath: mountPath }]
          probes: [
            { type: 'Startup', httpGet: { path: '/health', port: 7878 }, periodSeconds: 5, failureThreshold: 120 }
            { type: 'Readiness', httpGet: { path: '/health', port: 7878 }, periodSeconds: 10 }
            { type: 'Liveness', httpGet: { path: '/health', port: 7878 }, periodSeconds: 20, failureThreshold: 6 }
          ]
        }
      ]
      volumes: [{ name: 'holos', storageType: 'NfsAzureFile', storageName: envStorage.name }]
      scale: {
        minReplicas: writer ? 1 : minReplicas
        maxReplicas: writer ? 1 : maxReplicas
      }
    }
  }
}

// Built-in authentication: every request must carry a valid Entra ID token for this
// application, or is refused before it reaches the container.
resource auth 'Microsoft.App/containerApps/authConfigs@2025-01-01' = if (authEnabled) {
  parent: app
  name: 'current'
  properties: {
    platform: { enabled: true }
    globalValidation: { unauthenticatedClientAction: 'Return401' }
    identityProviders: {
      azureActiveDirectory: {
        enabled: true
        registration: {
          clientId: entraClientId
          openIdIssuer: '${environment().authentication.loginEndpoint}${entraTenantId}/v2.0'
        }
        validation: {
          allowedAudiences: [entraClientId, 'api://${entraClientId}']
        }
      }
    }
  }
}

output endpoint string = 'https://${app.properties.configuration.ingress.fqdn}'
output storageAccount string = storage.name
output shareMount string = '${storage.name}.file.${environment().suffixes.storage}:/${storage.name}/${share.name}'
