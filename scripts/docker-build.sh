#!/usr/bin/env bash
# ──── سكربت بناء وتشغيل Nexora ────
# الاستخدام: ./scripts/docker-build.sh [build|up|down|logs|clean]

set -euo pipefail

cd "$(dirname "$0")/.."

CMD="${1:-up}"

case "$CMD" in
    build)
        echo "🏗️  بناء الصور..."
        docker compose build --progress=plain
        ;;
    up)
        echo "🚀 تشغيل جميع الخدمات..."
        docker compose up -d --build
        echo ""
        echo "✅ الخدمات تعمل:"
        echo "   • Frontend:  http://localhost:3000"
        echo "   • Backend:   http://localhost:8080"
        echo "   • Postgres:  localhost:5432"
        echo "   • Prometheus: http://localhost:9090"
        echo "   • Grafana:   http://localhost:3001  (admin/admin)"
        ;;
    down)
        echo "🛑 إيقاف الخدمات..."
        docker compose down
        ;;
    logs)
        SVC="${2:-}"
        if [ -n "$SVC" ]; then
            docker compose logs -f "$SVC"
        else
            docker compose logs -f
        fi
        ;;
    clean)
        echo "🧹 تنظيف كل البيانات (volumes + images)..."
        docker compose down -v --rmi local
        ;;
    rebuild)
        echo "🔄 إعادة بناء بدون cache..."
        docker compose build --no-cache
        docker compose up -d
        ;;
    status)
        docker compose ps
        ;;
    *)
        echo "استخدام: $0 {build|up|down|logs|clean|rebuild|status}"
        echo ""
        echo "أوامر:"
        echo "  build    بناء الصور دون تشغيل"
        echo "  up       بناء + تشغيل (افتراضي)"
        echo "  down     إيقاف الخدمات"
        echo "  logs     عرض السجلات (اختياري: اسم الخدمة)"
        echo "  clean    حذف الكل (volumes + images)"
        echo "  rebuild  إعادة بناء كامل بدون cache"
        echo "  status   حالة الخدمات"
        exit 1
        ;;
esac
