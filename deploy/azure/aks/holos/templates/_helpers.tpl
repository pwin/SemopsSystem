{{- define "holos.fullname" -}}
{{- printf "%s" .Release.Name | trunc 50 | trimSuffix "-" -}}
{{- end -}}

{{- define "holos.labels" -}}
app.kubernetes.io/name: holos
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "holos.image" -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}
{{- end -}}

{{/* The server arguments every pod shares; the store and its mode are added per role. */}}
{{- define "holos.serverArgs" -}}
- --listen
- {{ printf "0.0.0.0:%v" .Values.server.port | quote }}
- --timeout
- {{ .Values.server.timeout | quote }}
- --max-body
- {{ .Values.server.maxBody | quote }}
- --max-query-memory
- {{ .Values.server.maxQueryMemory | quote }}
{{- if not .Values.server.ui }}
- --no-ui
{{- end }}
{{- if eq .Values.identity.mode "forwarded" }}
- --trust-forwarded-identity
{{- else if ne .Values.identity.mode "none" }}
{{- fail (printf "identity.mode must be none or forwarded, not %q" .Values.identity.mode) }}
{{- end }}
{{- if .Values.server.placetimeBase }}
- --placetime-base
- {{ .Values.server.placetimeBase | quote }}
{{- else if .Values.server.placetime }}
- --placetime
{{- end }}
{{- range .Values.server.extraArgs }}
- {{ . | quote }}
{{- end }}
{{- end -}}

{{- define "holos.probes" -}}
startupProbe:
  httpGet: { path: /health, port: http }
  periodSeconds: 5
  failureThreshold: 120
readinessProbe:
  httpGet: { path: /health, port: http }
  periodSeconds: 10
livenessProbe:
  httpGet: { path: /health, port: http }
  periodSeconds: 20
  failureThreshold: 6
{{- end -}}
