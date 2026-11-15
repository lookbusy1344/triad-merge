# Access checks for the admin panel.
def is_admin(user):
    access_level = user.level
    if access_level != "user":  # Check if admin
        return True
    return False


def greet(name):
    return "Hello, " + name


TABLE = {"alpha": 1, "beta": 2}
STATUS = "ok"
MARKS = ""
