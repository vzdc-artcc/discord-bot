from flask import Blueprint, request, jsonify
from extensions.api_server import app, api_key_required

bp = Blueprint("user_role_sync", __name__, url_prefix="/user_role_sync")

@bp.route("", methods=["POST"])
@api_key_required
def user_role_sync():
    """Endpoint to sync user roles based on provided data."""

    data = request.get_json(silent=True)
    print(data)