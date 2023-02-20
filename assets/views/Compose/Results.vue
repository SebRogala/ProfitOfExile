<template>
    <v-row v-show="results.totalTimeInMinutes" class="mt-4 mb-4">
        <v-col
            cols="3"
        >
            <p>Time: {{ results.totalTimeInMinutes }} min</p>
        </v-col>
        <v-col
            cols="3"
        >
            <p>Chaos per hour: {{ results.chaosPerHour }}</p>
        </v-col>
        <v-col
            cols="3"
        >
            <p>Div per hour: {{ results.divPerHour }}</p>
        </v-col>
        <v-col
            cols="3"
        >
            <p>Profit: {{ results.profit }} (chaos)</p>
        </v-col>
    </v-row>

    <v-table density="compact" v-show="results.items">
        <thead>
        <tr>
            <th class="text-left">
                Name
            </th>
            <th class="text-left">
                Singular price
            </th>
            <th class="text-left">
                Quantity
            </th>
            <th class="text-left">
                Sum (chaos)
            </th>
        </tr>
        </thead>
        <tbody>
        <tr
            v-for="item in results.items"
            :key="item.name"
        >
            <td>{{ item.name }}</td>
            <td>{{ item.singularPrice }}</td>
            <td>{{ item.quantity }}</td>
            <td>{{ item.summedPrice }}</td>
        </tr>
        </tbody>
    </v-table>
</template>

<script>
export default {
    name: 'Results',
    props: {
        results: Object
    },
}
</script>
