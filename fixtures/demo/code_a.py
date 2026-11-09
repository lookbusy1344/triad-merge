# ======================================================================
# TASK: Iris (ACs 3.1-3.3)
# ======================================================================

# Import libraries we need
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import LabelEncoder

# Load the Iris dataset
df = sns.load_dataset("iris")

 # Display the first few rows of the dataset
print(df.head())

 # Plot pairplot to visualise the data
sns.pairplot(df, hue="species")
plt.show()
# Encode the species column
label_encoder = LabelEncoder()
df['species'] = label_encoder.fit_transform(df['species'])

 # Split the data into features and target
X = df.drop(columns=['species']).values
y = df['species'].values

 # Split the data into training, validation, and test sets
X_train, X_temp, y_train, y_temp = train_test_split(X, y, test_size=0.3, random_state=42)
X_val, X_test, y_val, y_test = train_test_split(X_temp, y_temp, test_size=0.5, random_state=42)

print(f"Training set size: {X_train.shape}")
print(f"Validation set size: {X_val.shape}")
print(f"Test set size: {X_test.shape}")
def evaluate(model, X, y):
    A2 = model.forward(X)
    predictions = np.argmax(A2, axis=1)
    accuracy = np.mean(predictions == y)
    return accuracy
# Create class
class SimpleNN:

    def __init__(self, input_size, hidden_size, output_size):
        self.W1 = np.random.randn(input_size, hidden_size) * 0.01
        self.b1 = np.zeros((1, hidden_size))
        self.W2 = np.random.randn(hidden_size, output_size) * 0.01
        self.b2 = np.zeros((1, output_size))

    def sigmoid(self, Z):
        return 1 / (1 + np.exp(-Z))

    def sigmoid_derivative(self, Z):
        return Z * (1 - Z)

    def softmax(self, Z):
        expZ = np.exp(Z - np.max(Z))
        return expZ / expZ.sum(axis=1, keepdims=True)

    def forward(self, X):
        self.Z1 = np.dot(X, self.W1) + self.b1
        self.A1 = self.sigmoid(self.Z1)
        self.Z2 = np.dot(self.A1, self.W2) + self.b2
        self.A2 = self.softmax(self.Z2)
        return self.A2

    def compute_loss(self, Y, A2):
        m = Y.shape[0]
        log_likelihood = -np.log(A2[range(m), Y])
        loss = np.sum(log_likelihood) / m
        return loss

    def backward(self, X, Y, learning_rate):
        m = X.shape[0]
        Y_one_hot = np.zeros((m, 3))
        Y_one_hot[np.arange(m), Y] = 1

        dZ2 = self.A2 - Y_one_hot
        dW2 = np.dot(self.A1.T, dZ2) / m
        db2 = np.sum(dZ2, axis=0, keepdims=True) / m

        dA1 = np.dot(dZ2, self.W2.T)
        dZ1 = dA1 * self.sigmoid_derivative(self.A1)
        dW1 = np.dot(X.T, dZ1) / m
        db1 = np.sum(dZ1, axis=0, keepdims=True) / m

        self.W1 -= learning_rate * dW1
        self.b1 -= learning_rate * db1
        self.W2 -= learning_rate * dW2
        self.b2 -= learning_rate * db2

    def train(self, X, Y, epochs, learning_rate):
        for epoch in range(epochs):
            A2 = self.forward(X)
            loss = self.compute_loss(Y, A2)
            self.backward(X, Y, learning_rate)

            if epoch % 100 == 0:
                print(f"Epoch {epoch}, Loss: {loss}")

# Initialise and train the model
input_size = X_train.shape[1]
hidden_size = 10
output_size = 3
epochs = 1000
learning_rates = [0.01, 0.05, 0.1, 0.2]

for lr in learning_rates:
    model = SimpleNN(input_size, hidden_size, output_size)
    model.train(X_train, y_train, epochs, lr)
    val_accuracy = evaluate(model, X_val, y_val)
    print(f"Learning Rate: {lr}, Validation Accuracy: {val_accuracy:.2f}")

learning_rate = 0.1
model = SimpleNN(input_size, hidden_size, output_size)
model.train(X_train, y_train, epochs, learning_rate)
# Validate the model

val_accuracy = evaluate(model, X_val, y_val)
print(f"Validation Accuracy: {val_accuracy * 100:.2f}%")
# Test the model
test_accuracy = evaluate(model, X_test, y_test)
print(f"Test Accuracy: {test_accuracy * 100:.2f}%")

# Inference
def predict(model, X):
    A2 = model.forward(X)
    predictions = np.argmax(A2, axis=1)
    return predictions

# Make predictions on new data
new_data = np.array([[5.1, 3.5, 1.4, 0.2]])
predictions = predict(model, new_data)
predicted_species = label_encoder.inverse_transform(predictions)
print(f"Predicted species: {predicted_species[0]}")

# ======================================================================
# TASK: Synthetic data (AC 3.1)
# ======================================================================

# Import libraries we need and generate data
import numpy as np
from sklearn.datasets import make_classification
from sklearn.model_selection import train_test_split

# Generate a synthetic dataset
X, y = make_classification(n_samples=1000, n_features=20, n_classes=2, random_state=42)

# Split the dataset into training and testing sets
X_train, X_test, y_train, y_test = train_test_split(X, y, test_size=0.2, random_state=42)

# Print the shapes
print(f"Training set size: {X_train.shape}")
print(f"Testing set size: {X_test.shape}")
class SimpleNN:
    def __init__(self, input_size, hidden_size, output_size):
        self.W1 = np.random.randn(input_size, hidden_size) * 0.01
        self.b1 = np.zeros((1, hidden_size))
        self.W2 = np.random.randn(hidden_size, output_size) * 0.01
        self.b2 = np.zeros((1, output_size))

    def sigmoid(self, Z):
        return 1 / (1 + np.exp(-Z))

    def sigmoid_derivative(self, Z):
        return Z * (1 - Z)

    def softmax(self, Z):
        expZ = np.exp(Z - np.max(Z, axis=1, keepdims=True))
        return expZ / np.sum(expZ, axis=1, keepdims=True)

    def forward(self, X):
        self.Z1 = np.dot(X, self.W1) + self.b1
        self.A1 = self.sigmoid(self.Z1)
        self.Z2 = np.dot(self.A1, self.W2) + self.b2
        self.A2 = self.softmax(self.Z2)
        return self.A2

    def compute_loss(self, Y):
        m = Y.shape[0]
        log_likelihood = -np.log(self.A2[range(m), Y])
        loss = np.sum(log_likelihood) / m
        return loss

    def backward(self, X, Y, learning_rate):
        m = X.shape[0]
        Y_one_hot = np.zeros((m, 2))
        Y_one_hot[np.arange(m), Y] = 1

        dZ2 = self.A2 - Y_one_hot
        dW2 = np.dot(self.A1.T, dZ2) / m
        db2 = np.sum(dZ2, axis=0, keepdims=True) / m

        dA1 = np.dot(dZ2, self.W2.T)
        dZ1 = dA1 * self.sigmoid_derivative(self.A1)
        dW1 = np.dot(X.T, dZ1) / m
        db1 = np.sum(dZ1, axis=0, keepdims=True) / m

        self.W1 -= learning_rate * dW1
        self.b1 -= learning_rate * db1
        self.W2 -= learning_rate * dW2
        self.b2 -= learning_rate * db2

    def train(self, X, Y, epochs, learning_rate):
        for epoch in range(epochs):
            self.forward(X)
            loss = self.compute_loss(Y)
            self.backward(X, Y, learning_rate)
            if epoch % 100 == 0:
                print(f"Epoch {epoch}, Loss: {loss:.4f}")

    def predict(self, X):
        return np.argmax(self.forward(X), axis=1)
# Initialise and train the model
input_size = X_train.shape[1]
hidden_size = 10
output_size = 2
epochs = 1000
learning_rates = [0.01, 0.05, 0.1, 0.2]

for lr in learning_rates:
    model = SimpleNN(input_size, hidden_size, output_size)
    model.train(X_train, y_train, epochs, lr)
    # val_accuracy = evaluate(model, X_val, y_val)
    print(f"Learning Rate: {lr}")

learning_rate = 0.1
model = SimpleNN(input_size, hidden_size, output_size)
model.train(X_train, y_train, epochs, learning_rate)
# Evaluate the model
def accuracy(y_true, y_pred):
    return np.mean(y_true == y_pred)

train_acc = accuracy(y_train, model.predict(X_train))
test_acc = accuracy(y_test, model.predict(X_test))

print(f"Training Accuracy: {train_acc * 100:.2f}%")
print(f"Testing Accuracy: {test_acc * 100:.2f}%")

# ======================================================================
# TASK: ResNet / CIFAR-10 (ACs 3.3, 4.1-4.7)
# ======================================================================

import torch
import torchvision.transforms as transforms
from torchvision.datasets import CIFAR10
from torch.utils.data import DataLoader, random_split
# Define data augmentation and normalisation transformations
transform = transforms.Compose([
    transforms.RandomResizedCrop(224),
    transforms.RandomHorizontalFlip(),
    transforms.RandomRotation(10),
    transforms.RandomVerticalFlip(),
    transforms.ToTensor(),
    transforms.Normalize((0.485, 0.456, 0.406), (0.229, 0.224, 0.225))
])
# Load CIFAR-10 dataset
dataset = CIFAR10(root='./data', train=True, download=True, transform=transform)
# Split dataset into training, validation, and testing sets
train_size = int(0.8 * len(dataset))
val_size = int(0.1 * len(dataset))
test_size = len(dataset) - train_size - val_size

train_dataset, val_dataset, test_dataset = random_split(dataset, [train_size, val_size, test_size])
# Create data loaders
train_loader = DataLoader(train_dataset, batch_size=32, shuffle=True)
val_loader = DataLoader(val_dataset, batch_size=32, shuffle=False)
test_loader = DataLoader(test_dataset, batch_size=32, shuffle=False)
import torchvision.models as models

print("Choose a training model:")
print("1 - ResNet18")
print("2 - ResNet34")

choice = input("Enter your choice (1 or 2): ")

if choice == "1":
    model = models.resnet18(pretrained=True)
    print("ResNet18 selected")
elif choice == "2":
    model = models.resnet34(pretrained=True)
    print("ResNet34 selected")
else:
    raise ValueError("Invalid choice. Please enter 1 or 2.")
# Modify the final layer to match the number of classes in CIFAR-10
num_ftrs = model.fc.in_features
model.fc = torch.nn.Linear(num_ftrs, 10)
import torch.optim as optim
import torch.nn as nn
from torch.cuda import is_available

# Check for GPU
device = torch.device("cuda:0" if is_available() else "cpu")
model = model.to(device)
# Loss function and optimizer
criterion = nn.CrossEntropyLoss()
optimizer = optim.SGD(model.parameters(), lr=0.001, momentum=0.9)
# Training loop
num_epochs = 10

for epoch in range(num_epochs):
    model.train()
    running_loss = 0.0
    for inputs, labels in train_loader:
        inputs, labels = inputs.to(device), labels.to(device)

        optimizer.zero_grad()

        outputs = model(inputs)
        loss = criterion(outputs, labels)
        loss.backward()
        optimizer.step()

        running_loss += loss.item()

    print(f"Epoch {epoch+1}/{num_epochs}, Loss: {running_loss/len(train_loader)}")
# Save the model checkpoint
torch.save(model.state_dict(), 'model.pth')

# Load the model checkpoint
model.load_state_dict(torch.load('model.pth'))
model.eval()
from PIL import Image
import requests
from io import BytesIO

url = "https://cdn.britannica.com/79/232779-050-6B0411D7/German-Shepherd-dog-Alsatian.jpg"
response = requests.get(url)
img = Image.open(BytesIO(response.content))
# Apply the same transformations as used in training
transform_inference = transforms.Compose([
    transforms.Resize(256),
    transforms.CenterCrop(224),
    transforms.ToTensor(),
    transforms.Normalize((0.485, 0.456, 0.406), (0.229, 0.224, 0.225))
])

img_t = transform_inference(img)
img_t = img_t.unsqueeze(0).to(device)  # Add batch dimension and move to GPU
# Inference
model.eval()
with torch.no_grad():
    output = model(img_t)
    _, predicted = torch.max(output, 1)
    print(f'Predicted class: {predicted.item()}')

# ======================================================================
# TASK: VGG / CIFAR-10 (ACs 3.2, 4.1-4.7)
# ======================================================================

import torch
import torchvision.models as models
import torchvision.transforms as transforms
from torchvision.datasets import CIFAR10
from torch.utils.data import DataLoader, random_split
from PIL import Image
import requests
from io import BytesIO
import torch.nn as nn
import torch.optim as optim

# AC 4.2 - Allow the user to choose the training model
print("Choose a training model:")
print("1 - VGG16")
print("2 - VGG19")

choice = input("Enter your choice (1 or 2): ")

if choice == "1":
    model = models.vgg16(pretrained=True)
    print("VGG16 selected")
elif choice == "2":
    model = models.vgg19(pretrained=True)
    print("VGG19 selected")
else:
    raise ValueError("Invalid choice. Please enter 1 or 2.")

# Modify the classifier to fit CIFAR-10
model.classifier[6] = nn.Linear(4096, 10)

# Move the model to GPU
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
model = model.to(device)

# Freeze the feature extractor layers
for param in model.features.parameters():
    param.requires_grad = False

# Define transformations
transform_train = transforms.Compose([
    transforms.RandomHorizontalFlip(),
    transforms.RandomCrop(32, padding=4),
    transforms.Resize(224),
    transforms.ToTensor(),
    transforms.Normalize((0.485, 0.456, 0.406), (0.229, 0.224, 0.225))
])

transform_test = transforms.Compose([
    transforms.Resize(224),
    transforms.ToTensor(),
    transforms.Normalize((0.485, 0.456, 0.406), (0.229, 0.224, 0.225))
])

# Load datasets
train_dataset = CIFAR10(root='./data', train=True, download=True, transform=transform_train)
test_dataset = CIFAR10(root='./data', train=False, download=True, transform=transform_test)

# Split the training dataset into training and validation sets
train_size = int(0.8 * len(train_dataset))
val_size = len(train_dataset) - train_size
train_dataset, val_dataset = random_split(train_dataset, [train_size, val_size])

# Create data loaders
train_loader = DataLoader(train_dataset, batch_size=64, shuffle=True, num_workers=4)
val_loader = DataLoader(val_dataset, batch_size=64, shuffle=False, num_workers=4)
test_loader = DataLoader(test_dataset, batch_size=64, shuffle=False, num_workers=4)
# Define the optimiser and learning rate scheduler
optimizer = optim.SGD(model.classifier.parameters(), lr=0.001, momentum=0.9)
scheduler = optim.lr_scheduler.StepLR(optimizer, step_size=7, gamma=0.1)

# Define the loss function
criterion = nn.CrossEntropyLoss()

# Training loop with early stopping to reduce training time
num_epochs = 5
best_val_loss = float('inf')
patience = 3
trigger_times = 0

for epoch in range(num_epochs):
    model.train()
    running_loss = 0.0
    for inputs, labels in train_loader:
        inputs, labels = inputs.to(device), labels.to(device)

        optimizer.zero_grad()

        outputs = model(inputs)
        loss = criterion(outputs, labels)
        loss.backward()
        optimizer.step()

        running_loss += loss.item()

    model.eval()
    val_loss = 0.0
    with torch.no_grad():
        for inputs, labels in val_loader:
            inputs, labels = inputs.to(device), labels.to(device)
            outputs = model(inputs)
            loss = criterion(outputs, labels)
            val_loss += loss.item()

    val_loss /= len(val_loader)

    if val_loss < best_val_loss:
        best_val_loss = val_loss
        trigger_times = 0
        torch.save(model.state_dict(), 'best_model.pth')
    else:
        trigger_times += 1
        if trigger_times >= patience:
            print(f"Early stopping at epoch {epoch+1}")
            break

    print(f"Epoch {epoch+1}/{num_epochs}, Training Loss: {running_loss/len(train_loader)}, Validation Loss: {val_loss}")
    scheduler.step()
# Load the best model
model.load_state_dict(torch.load('best_model.pth'))

# Replace this URL with the URL of an actual image from the CIFAR-10 dataset
url = "https://www.cs.toronto.edu/~kriz/cifar-10-sample/ship7.png"
response = requests.get(url)
img = Image.open(BytesIO(response.content))

# Convert to RGB if the image is in grayscale
if img.mode != 'RGB':
    img = img.convert('RGB')

# Apply the same transformations as used in training
transform_inference = transforms.Compose([
    transforms.Resize(256),
    transforms.CenterCrop(224),
    transforms.ToTensor(),
    transforms.Normalize((0.485, 0.456, 0.406), (0.229, 0.224, 0.225))
])

img_t = transform_inference(img)
img_t = img_t.unsqueeze(0).to(device)  # Add batch dimension and move to GPU

# Inference
model.eval()
with torch.no_grad():
    output = model(img_t)
    _, predicted = torch.max(output, 1)
    predicted_class = predicted.item()

# CIFAR-10 class labels
class_labels = [
    'Airplane', 'Automobile', 'Bird', 'Cat', 'Deer', 'Dog',
    'Frog', 'Horse', 'Ship', 'Truck'
]

# Print the predicted class label
print(f'Predicted class: {class_labels[predicted_class]}')
