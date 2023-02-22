<template>
    <v-row class="pt-6 pb-6">
        <v-col
            cols="3"
        >
            <p>Time: {{ $filters.number(results.totalTimeInMinutes) }} min</p>
        </v-col>
        <v-col
            cols="3"
        >
            <p>Chaos per hour: {{ $filters.number(results.chaosPerHour) }}</p>
        </v-col>
        <v-col
            cols="3"
        >
            <p>Div per hour: {{ $filters.number(results.divPerHour) }}</p>
        </v-col>
        <v-col
            cols="3"
        >
            <p>Profit: {{ $filters.number(results.profit) }} (chaos)</p>
        </v-col>
    </v-row>

    <v-table
        class="pb-6"
        density="compact"
    >
        <thead>
        <tr class="font-weight-bold">
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
            <td>{{ $filters.number(item.singularPrice) }}</td>
            <td>{{ item.quantity }}</td>
            <td>{{ $filters.number(item.summedPrice) }}</td>
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
